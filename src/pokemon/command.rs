use crate::{
    Context, Error,
    image::{alpha_to_mask, background, encode_webp},
};
use futures::StreamExt;
use image::ImageReader;
use poise::serenity_prelude as serenity;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::io::Cursor;
use wana_kana::ConvertJapanese;

/// ポケモンのシルエットクイズができます。
#[poise::command(slash_command, guild_only)] // future cannot be sent between threads safely
pub async fn dareda(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;

    let pokemon = ctx
        .data()
        .pokemon_api
        .random(&mut StdRng::from_entropy())
        .await;
    let pokemon = match pokemon {
        Ok(pokemon) => pokemon,
        Err(err) => {
            tracing::error!("fetch pokemon error: {err}");
            ctx.reply("ポケモンが見つかりませんでした").await?;

            return Ok(());
        }
    };

    let Some(pokemon_image) = pokemon
        .image_bytes()
        .await
        .inspect_err(|err| {
            tracing::error!("fetch pokemon image error: {err}");
        })
        .ok()
        .flatten()
        .and_then(|bytes| {
            ImageReader::new(Cursor::new(bytes.as_ref()))
                .with_guessed_format()
                .ok()?
                .decode()
                .inspect_err(|err| {
                    tracing::error!("decode pokemon image error: {err}");
                })
                .ok()
        })
    else {
        ctx.reply("ポケモンの画像が取得できませんでした").await?;

        return Ok(());
    };
    let Some(name) = pokemon.name().await? else {
        ctx.reply("ポケモンの名前が取得できませんでした").await?;

        return Ok(());
    };
    let normalized_name = name.to_katakana();
    let flavor_text = pokemon
        .flavor_text()
        .await?
        .map(|f| format!("\n説明：{}", f.replace('\n', "　")))
        .unwrap_or("".to_owned());
    let correct = format!(
        "{name}でした！\n\n全国図鑑番号：{id}{flavor_text}",
        id = pokemon.id
    );
    let result_image = background(&pokemon_image);
    // TODO: ファイル名
    let attachment = serenity::CreateAttachment::bytes(encode_webp(&result_image)?, "pokemon.webp");
    let silhouette_image = alpha_to_mask(&pokemon_image);
    let data = ctx.data();
    let reply = ctx.send(
        poise::CreateReply::default()
            .content(
                "だーれだ？\n".to_owned()
                    + "返信で答えてみよう（ひらがな/カタカナ/ローマ字）\n"
                    + &format!("制限時間は{}分、{}回まで回答できるよ\n", data.config.pokemon.time_limit.as_secs() / 60, data.config.pokemon.max_retry)
                    + "どうしてもわかんないよ！ってときは「ギブアップ」って返信してね（コマンド実行者のみ）"
        )
            .attachment(serenity::CreateAttachment::bytes(
                encode_webp(&silhouette_image)?,
                "pokemon.webp",
            )),
    )
    .await?;
    let reply_message = reply.message().await?;
    let reply_message_id = reply_message.id;

    let mut collector = ctx
        .channel_id()
        .await_reply(ctx)
        .filter(move |m| {
            m.message_reference
                .as_ref()
                .and_then(|r| r.message_id.as_ref())
                == Some(&reply_message_id)
        })
        .timeout(data.config.pokemon.time_limit)
        .stream();
    let mut retry = 0;

    while let Some(m) = collector.next().await {
        let answer = m.content.trim().to_katakana();

        if answer == normalized_name {
            ctx.channel_id()
                .send_message(
                    ctx,
                    serenity::CreateMessage::new()
                        .add_file(attachment)
                        .reference_message(&m)
                        .content(format!("あたり！\n{correct}"))
                        .allowed_mentions(
                            serenity::CreateAllowedMentions::new()
                                .replied_user(false)
                                .everyone(false)
                                .all_users(false)
                                .all_roles(false),
                        ),
                )
                .await?;

            return Ok(());
        }

        if answer == "ギブアップ" && m.author.id == ctx.author().id {
            ctx.channel_id()
                .send_message(
                    ctx,
                    serenity::CreateMessage::new()
                        .add_file(attachment)
                        .reference_message(&m)
                        .content(format!("ざんねん！\n{correct}"))
                        .allowed_mentions(
                            serenity::CreateAllowedMentions::new()
                                .replied_user(false)
                                .everyone(false)
                                .all_users(false)
                                .all_roles(false),
                        ),
                )
                .await?;

            return Ok(());
        }

        m.reply(ctx, "はずれ！").await?;

        retry += 1;

        if should_end(retry, data.config.pokemon.max_retry) {
            ctx.channel_id()
                .send_message(
                    ctx,
                    serenity::CreateMessage::new()
                        .add_file(attachment)
                        .reference_message(&m)
                        .content(format!("解答可能回数がなくなりました\n{correct}"))
                        .allowed_mentions(
                            serenity::CreateAllowedMentions::new()
                                .replied_user(false)
                                .everyone(false)
                                .all_users(false)
                                .all_roles(false),
                        ),
                )
                .await?;

            return Ok(());
        }
    }

    ctx.channel_id()
        .send_message(
            ctx,
            serenity::CreateMessage::new()
                .add_file(attachment)
                .reference_message((ctx.channel_id(), reply_message_id))
                .content(format!("時間切れ！\n{correct}"))
                .allowed_mentions(
                    serenity::CreateAllowedMentions::new()
                        .replied_user(false)
                        .everyone(false)
                        .all_users(false)
                        .all_roles(false),
                ),
        )
        .await?;

    Ok(())
}

/// 回答可能回数を使い切ったかどうかを返す。
fn should_end(retry: usize, max_retry: usize) -> bool {
    retry >= max_retry
}

#[cfg(test)]
mod tests {
    use super::should_end;

    #[test]
    fn should_end_at_max_retry() {
        assert!(should_end(5, 5));
    }

    #[test]
    fn should_not_end_before_max_retry() {
        assert!(!should_end(4, 5));
    }
}
