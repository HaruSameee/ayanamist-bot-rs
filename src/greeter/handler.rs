use crate::{Data, Error, db};
use ::serenity::all::Mentionable;
use poise::serenity_prelude as serenity;

pub async fn handle_member_add(
    ctx: &serenity::Context,
    data: &Data,
    new_member: &serenity::Member,
) -> Result<(), Error> {
    if new_member.guild_id != data.config.guild.guild_id {
        return Ok(());
    }

    let joined_at = new_member
        .joined_at
        .map_or_else(db::now_unix, |t| t.timestamp());
    if let Err(err) = db::insert_member_join(
        &data.db,
        new_member.user.id.get(),
        joined_at,
        new_member.user.created_at().timestamp(),
    )
    .await
    {
        tracing::error!("failed to record member join: {err}");
    }

    data.config
        .greeter
        .channel_id
        .send_message(
            ctx,
            serenity::CreateMessage::new()
                .content(format!(
                    "{} ({}) join\njoin server {joined}\njoin discord <t:{created}:F>",
                    new_member.mention(),
                    new_member.user.name,
                    joined = new_member
                        .joined_at
                        .map_or("不明".to_owned(), |t| format!("<t:{}:F>", t.timestamp())),
                    created = new_member.user.created_at().timestamp()
                ))
                .allowed_mentions(
                    serenity::CreateAllowedMentions::new()
                        .all_roles(false)
                        .all_users(false)
                        .everyone(false)
                        .replied_user(false),
                ),
        )
        .await?;

    Ok(())
}
