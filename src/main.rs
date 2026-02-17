mod bot;
mod config;
mod greeter;
mod http;
mod image;
mod logger;
mod madomagi;
mod pokemon;
mod proxy;
mod verify;

use config::Config;
use poise::serenity_prelude as serenity;
use std::env;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

#[derive(Clone)]
struct Data {
    config: Config,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // TODO: .expect()また.ok()にする
    dotenvy::dotenv().unwrap();

    logger::init_tracing_subscriber().expect("setting subscriber failed");

    let config = Config::load().map_err(|e| format!("config.toml の読み込みに失敗: {e}"))?;

    // TODO: .expect()にする
    let token = env::var("DISCORD_BOT_TOKEN").unwrap();

    let config_for_setup = config.clone();

    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILD_MEMBERS;

    let commands = {
        let mut commands = vec![
            bot::command::ping(),
            proxy::command::proxy(),
            proxy::command::proxycheck(),
            pokemon::command::dareda(),
            madomagi::command::dj(),
            madomagi::command::sayakais(),
        ];

        let captcha_perm = serenity::Permissions::from_name(
            &config
                .verify
                .captcha_default_permission
                .trim()
                .to_ascii_uppercase(),
        )
        .unwrap_or(serenity::Permissions::ADMINISTRATOR);
        let mut captcha_command = verify::command::captcha();

        captcha_command.default_member_permissions = captcha_perm;

        commands.push(captcha_command);

        commands
    };

    let options = poise::FrameworkOptions {
        commands,
        on_error: |err: poise::FrameworkError<'_, Data, Error>| {
            Box::pin(async move {
                tracing::warn!("Global error: {err}");
            })
        },

        event_handler: |ctx, event, _framework: poise::FrameworkContext<'_, Data, _>, data| {
            Box::pin(async move {
                if let serenity::FullEvent::GuildMemberAddition { new_member } = event {
                    greeter::handler::handle_member_add(ctx, data, new_member).await?;
                }

                if let serenity::FullEvent::InteractionCreate { interaction } = event
                    && let serenity::Interaction::Component(comp) = interaction
                {
                    let custom_id = comp.data.custom_id.as_str();
                    let namespace = custom_id.split(':').next().unwrap_or("");

                    match namespace {
                        "captcha" => verify::handler::handle_component(ctx, data, comp).await?,
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
                tracing::info!("Logged in as {}", ready.user.name);
                tracing::debug!(
                    "Intents: {}",
                    intents
                        .iter_names()
                        .map(|(s, _)| s)
                        .collect::<Vec<_>>()
                        .join(", ")
                );

                poise::builtins::register_in_guild(
                    ctx,
                    &framework.options().commands,
                    config.guild.guild_id,
                )
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
