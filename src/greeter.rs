use crate::{Data, Error};
use ::serenity::all::Mentionable;
use poise::serenity_prelude as serenity;

pub async fn handle_member_add(
    ctx: &serenity::Context,
    data: &Data,
    new_member: &serenity::Member,
) -> Result<(), Error> {
    if new_member.guild_id != data.config.discord.guild_id {
        return Ok(());
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
