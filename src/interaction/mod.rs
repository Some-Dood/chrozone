mod error;

use chrono::Timelike;
use twilight_model::{
    application::interaction::{application_command::CommandData, Interaction},
    http::interaction::InteractionResponse,
};

fn create_parsed_from_now() -> chrono::format::ParseResult<chrono::format::Parsed> {
    let now = chrono::Local::now();

    use chrono::Datelike;
    let mut parsed = chrono::format::Parsed::new();
    parsed.set_year(now.year().into())?;
    parsed.set_month(now.month().into())?;
    parsed.set_day(now.day().into())?;
    parsed.set_hour(now.hour().into())?;
    parsed.set_second(now.second().into())?;

    Ok(parsed)
}

fn on_app_command(data: CommandData) -> error::Result<InteractionResponse> {
    use alloc::string::ToString;
    use chrono::format::Parsed;
    use twilight_model::{
        application::interaction::application_command::{CommandDataOption, CommandOptionValue},
        channel::message::MessageFlags,
        http::interaction::{InteractionResponseData, InteractionResponseType::ChannelMessageWithSource},
    };

    // TODO: Verify command ID.

    // Set default epoch arguments
    let mut tz = chrono_tz::Tz::UTC;
    let mut parsed = create_parsed_from_now().map_err(|_| error::Error::Fatal)?;

    // Parse each argument
    for CommandDataOption { name, value } in data.options {
        log::info!("Received argument {name} as {value:?}.");
        let setter = match name.as_str() {
            "timezone" => {
                let text = if let CommandOptionValue::String(text) = value {
                    text.into_boxed_str()
                } else {
                    log::error!("Non-string command option value encountered for timezone.");
                    return Err(error::Error::Fatal);
                };
                tz = match text.parse::<chrono_tz::Tz>() {
                    Ok(timezone) => timezone,
                    Err(err) => {
                        log::error!("Failed to set timezone: {err}.");
                        return Err(error::Error::UnknownTimezone);
                    }
                };
                continue;
            }
            "year" => Parsed::set_year,
            "month" => Parsed::set_month,
            "day" => Parsed::set_day,
            "hour" => Parsed::set_hour,
            "minute" => Parsed::set_minute,
            "secs" => Parsed::set_second,
            other => {
                log::error!("Unable to parse command name {other}.");
                return Err(error::Error::InvalidArgs)
            },
        };

        let num = if let CommandOptionValue::Integer(num) = value {
            num
        } else {
            log::error!("Incorrect command option value received.");
            return Err(error::Error::Fatal);
        };

        if let Err(err) = setter(&mut parsed, num) {
            log::error!("Failed to set {num} to parser: {err}.");
            return Err(error::Error::InvalidArgs);
        }
    }

    let timestamp = match parsed.to_datetime_with_timezone(&tz) {
        Ok(datetime) => datetime.timestamp(),
        Err(err) => {
            log::error!("Failed to create date-time: {err}.");
            return Err(error::Error::InvalidArgs);
        }
    };

    Ok(InteractionResponse {
        kind: ChannelMessageWithSource,
        data: Some(InteractionResponseData {
            content: Some(timestamp.to_string()),
            flags: Some(MessageFlags::EPHEMERAL),
            ..Default::default()
        }),
    })
}

fn on_autocomplete(data: CommandData) -> InteractionResponse {
    use alloc::borrow::ToOwned;
    use twilight_model::{
        application::{
            command::{CommandOptionChoice, CommandOptionType},
            interaction::application_command::{CommandDataOption, CommandOptionValue::Focused},
        },
        http::interaction::{InteractionResponseData, InteractionResponseType::ApplicationCommandAutocompleteResult},
    };

    // TODO: Verify command ID.

    let choices = data
        .options
        .into_iter()
        .find_map(|CommandDataOption { name, value }| match (name.as_str(), value) {
            ("timezone", Focused(comm, CommandOptionType::String)) => Some(comm.into_boxed_str()),
            _ => None,
        })
        .map(|query| crate::util::autocomplete_tz(&query, 25))
        .unwrap_or_default()
        .into_iter()
        .take(25)
        .map(|tz| {
            let choice = tz.to_owned();
            CommandOptionChoice::String { name: choice.clone(), name_localizations: None, value: choice }
        })
        .collect();
    log::info!("Generated autocompletions: {:?}", choices);

    InteractionResponse {
        kind: ApplicationCommandAutocompleteResult,
        data: Some(InteractionResponseData { choices: Some(choices), ..Default::default() }),
    }
}

fn try_respond(interaction: Interaction) -> error::Result<InteractionResponse> {
    use twilight_model::{
        application::interaction::{
            InteractionData,
            InteractionType::{ApplicationCommand, ApplicationCommandAutocomplete, Ping},
        },
        http::interaction::InteractionResponseType::Pong,
    };

    let is_comm = match interaction.kind {
        ApplicationCommand => true,
        ApplicationCommandAutocomplete => false,
        Ping => {
            log::info!("Received a ping.");
            return Ok(InteractionResponse { kind: Pong, data: None });
        }
        _ => {
            log::error!("Received unsupported interaction type.");
            return Err(error::Error::UnsupportedInteractionType);
        }
    };

    let data = match interaction.data.ok_or(error::Error::MissingPayload)? {
        InteractionData::ApplicationCommand(data) => *data,
        _ => {
            log::error!("Missing payload from application command invocation.");
            return Err(error::Error::Fatal);
        }
    };

    Ok(if is_comm {
        log::info!("Received application command.");
        on_app_command(data)?
    } else {
        log::info!("Received autocompletion request.");
        on_autocomplete(data)
    })
}

pub fn respond(interaction: Interaction) -> InteractionResponse {
    try_respond(interaction).unwrap_or_else(|err| {
        use alloc::string::ToString;
        use twilight_model::{
            channel::message::MessageFlags,
            http::interaction::{InteractionResponseData, InteractionResponseType::ChannelMessageWithSource},
        };
        InteractionResponse {
            kind: ChannelMessageWithSource,
            data: Some(InteractionResponseData {
                content: Some(err.to_string()),
                flags: Some(MessageFlags::EPHEMERAL),
                ..Default::default()
            }),
        }
    })
}
