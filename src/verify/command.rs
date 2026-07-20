use crate::verify::common::{COLOR_AQUA, GUIDE_IMAGE_URL};
use crate::verify::handler::START_ID;
use crate::{Context, Error};
use poise::serenity_prelude as serenity;

async fn is_staff(ctx: Context<'_>) -> Result<bool, Error> {
    let Some(member) = ctx.author_member().await else {
        return Ok(false);
    };
    Ok(member
        .roles
        .contains(&ctx.data().config.guild.staff_role_id))
}

/// 認証パネルを設置
#[poise::command(slash_command, guild_only)]
pub async fn captcha(ctx: Context<'_>) -> Result<(), Error> {
    if !is_staff(ctx).await? {
        ctx.send(
            poise::CreateReply::default()
                .content("権限がありません。")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    let embed = serenity::CreateEmbed::new()
        .color(COLOR_AQUA)
        .image(GUIDE_IMAGE_URL);

    let button = serenity::CreateButton::new(START_ID)
        .label("認証する")
        .style(serenity::ButtonStyle::Success);

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .components(vec![serenity::CreateActionRow::Buttons(vec![button])]),
    )
    .await?;

    Ok(())
}
