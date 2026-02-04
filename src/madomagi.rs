use crate::{Context, Error};

const SAYAKAIS_VIDEO_URL: &str = "https://cdn.discordapp.com/attachments/921625188444041249/936275774158278737/nicovideo-sm16800638_1082d67867b67a94b934949cb59932b6476495fa1551d86e50c27bd8b7e3e057.mp4";
const DJ_VIDEO_URL: &str = "https://cdn.discordapp.com/attachments/921625188444041249/936275839262277703/nicovideo-sm22591026_03cce4fa300fcd6dcc40866bc4d6d8ae7ddd8fb11500e74accdbc2516c9a2f32.mp4";

/// 魔法少女まどかマギカのコラ動画を見ることができます
#[poise::command(slash_command, guild_only, category = "まどマギ")]
pub async fn dj(ctx: Context<'_>) -> Result<(), Error> {
    ctx.reply(DJ_VIDEO_URL).await?;

    Ok(())
}

/// 魔法少女まどかマギカのコラ動画を見ることができます
#[poise::command(slash_command, guild_only, category = "まどマギ")]
pub async fn sayakais(ctx: Context<'_>) -> Result<(), Error> {
    ctx.reply(SAYAKAIS_VIDEO_URL).await?;

    Ok(())
}
