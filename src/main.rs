mod config;
mod greeter;
mod http;
mod image;
mod interaction;
mod pokemon;
mod proxy;
mod verify;

use config::Config;
use poise::serenity_prelude as serenity;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

#[derive(Clone)]
struct Data {
    config: Config,
}

/// pong
#[poise::command(slash_command, guild_only)]
async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("pong 🦀").await?;
    Ok(())
}

// 文字列やu64をPermissionsへ変換する
fn parse_permissions(s: &str) -> serenity::Permissions {
    let s = s.trim();

    // 数値をbitとして解釈する
    if let Ok(bits) = s.parse::<u64>() {
        return serenity::Permissions::from_bits_truncate(bits);
    }

    serenity::Permissions::from_name(&s.to_ascii_uppercase()).unwrap_or_else(|| {
        tracing::warn!("Unknown permission value in config: {}", s);

        serenity::Permissions::empty()
    })
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();

    let config = Config::load().map_err(|e| format!("config.toml の読み込みに失敗: {e}"))?;

    let token = config.discord.token.clone();
    let guild_id = serenity::GuildId::new(config.discord.guild_id);

    let config_for_setup = config.clone();

    let intents =
        serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT;

    let commands = {
        let mut commands = vec![
            ping(),
            proxy::command::proxy(),
            proxy::command::proxycheck(),
            pokemon::command::dareda(),
        ];

        // TODO: MANAGE_GUILDとBAN_MEMBERSなど複数のパーミッションを解釈できないがこれでいいのか？
        let captcha_perm = parse_permissions(&config.discord.captcha_default_permission);
        let mut captcha_command = verify::captcha::captcha();

        captcha_command.default_member_permissions = captcha_perm;

        commands.push(captcha_command);

        commands
    };

    let options = poise::FrameworkOptions {
        commands,
        // poiseの二重応答対策
        on_error: |err: poise::FrameworkError<'_, Data, Error>| {
            Box::pin(async move {
                match err {
                    poise::FrameworkError::CommandCheckFailed { .. } => {}
                    other => {
                        if let Err(e) = poise::builtins::on_error(other).await {
                            let message = e.to_string();
                            if message.contains("Interaction has already been acknowledged")
                                || message.contains("Unknown interaction")
                            {
                                tracing::debug!(
                                    "Skipped error reply for interaction response: {}",
                                    message
                                );
                            } else {
                                tracing::error!("Fatal error while sending error message: {}", e);
                            }
                        }
                    }
                }
            })
        },

        event_handler: |ctx, event, _framework: poise::FrameworkContext<'_, Data, _>, data| {
            Box::pin(async move {
                if let serenity::FullEvent::GuildMemberAddition { new_member } = event {
                    greeter::handle_member_add(ctx, data, new_member).await?;
                }

                if let serenity::FullEvent::InteractionCreate { interaction } = event
                    && let serenity::Interaction::Component(comp) = interaction
                {
                    let custom_id = comp.data.custom_id.as_str();
                    let namespace = custom_id.split(':').next().unwrap_or("");

                    match namespace {
                        "captcha" => verify::captcha::handle_component(ctx, data, comp).await?,
                        "proxy" => proxy::handler::handle_component(ctx, data, comp).await?,
                        _ => {
                            tracing::warn!("unknown component: {}", custom_id);
                        }
                    }
                }
                Ok(())
            })
        },
        ..Default::default()
    };

    let framework = poise::Framework::builder()
        .options(options)
        .setup(move |ctx, ready, framework| {
            let config = config_for_setup.clone();
            Box::pin(async move {
                println!("Logged in as {}", ready.user.name);

                poise::builtins::register_in_guild(ctx, &framework.options().commands, guild_id)
                    .await?;

                Ok(Data { config })
            })
        })
        .build();

    let mut client = serenity::Client::builder(token, intents)
        .framework(framework)
        .await?;

    client.start().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use poise::serenity_prelude as serenity;

    #[test]
    fn it_works() {
        assert_eq!(
            parse_permissions("BAN_MEMBERS"),
            serenity::Permissions::BAN_MEMBERS
        );
        assert_eq!(
            parse_permissions("INVALID_PERMISSION_NAME"),
            serenity::Permissions::empty()
        );
    }
}
