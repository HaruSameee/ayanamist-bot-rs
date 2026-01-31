mod config;
mod verify;

use poise::serenity_prelude as serenity;

use config::Config;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

#[derive(Clone)]
struct Data {
    config: Config,
}

#[poise::command(slash_command)]
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

    match s.to_ascii_uppercase().as_str() {
        "ADMINISTRATOR" => serenity::Permissions::ADMINISTRATOR,
        "MANAGE_GUILD" | "MANAGE_SERVER" => serenity::Permissions::MANAGE_GUILD,
        "MANAGE_ROLES" => serenity::Permissions::MANAGE_ROLES,
        "MANAGE_CHANNELS" => serenity::Permissions::MANAGE_CHANNELS,
        "KICK_MEMBERS" => serenity::Permissions::KICK_MEMBERS,
        "BAN_MEMBERS" => serenity::Permissions::BAN_MEMBERS,
        "MODERATE_MEMBERS" | "TIMEOUT_MEMBERS" => serenity::Permissions::MODERATE_MEMBERS,
        _ => serenity::Permissions::MANAGE_GUILD, 
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();

    let config = Config::load()
        .map_err(|e| format!("config.toml の読み込みに失敗: {e}"))?;

    let token = config.discord.token.clone();
    let guild_id = serenity::GuildId::new(config.discord.guild_id);

    let config_for_setup = config.clone();

    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::MESSAGE_CONTENT;

    let options = poise::FrameworkOptions {
        commands: vec![
            ping(),
            verify::captcha::captcha(),
        ],

        // poiseの二重応答対策
        on_error: |err| {
            Box::pin(async move {
                match err {
                    poise::FrameworkError::CommandCheckFailed { .. } => {
                       
                    }
                    other => {
                        poise::builtins::on_error(other).await;
                    }
                }
            })
        },

        event_handler: |ctx, event, _framework, data| {
            Box::pin(async move {
                if let serenity::FullEvent::InteractionCreate { interaction } = event {
                    if let serenity::Interaction::Component(comp) = interaction {
                        let _ = verify::captcha::handle_component(ctx, data, &comp).await;
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

let captcha_perm = parse_permissions(&config.discord.captcha_default_permission);

let mut cmds = Vec::new();

// ping
cmds.push(
    serenity::CreateCommand::new("ping")
        .description("pong")
        .dm_permission(false)
);

// captcha
cmds.push(
    serenity::CreateCommand::new("captcha")
        .description("認証パネルを設置")
        .default_member_permissions(captcha_perm)
        .dm_permission(false)
);

guild_id.set_commands(ctx.http.clone(), cmds).await?;


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
