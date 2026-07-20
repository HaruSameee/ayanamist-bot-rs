use crate::{
    Context, Error, db,
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
#[poise::command(slash_command, guild_only, subcommands("play", "ranking", "stats"))]
pub async fn dareda(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// ポケモンのシルエットクイズを始めます。
#[poise::command(slash_command, guild_only)] // future cannot be sent between threads safely
pub async fn play(ctx: Context<'_>) -> Result<(), Error> {
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

            if let Err(err) = db::insert_dareda_result(
                &data.db,
                m.author.id.get(),
                pokemon.id,
                true,
                retry as u32 + 1,
                db::now_unix(),
            )
            .await
            {
                tracing::error!("failed to record dareda result: {err}");
            }

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

            if let Err(err) = db::insert_dareda_result(
                &data.db,
                ctx.author().id.get(),
                pokemon.id,
                false,
                retry as u32,
                db::now_unix(),
            )
            .await
            {
                tracing::error!("failed to record dareda result: {err}");
            }

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

            if let Err(err) = db::insert_dareda_result(
                &data.db,
                ctx.author().id.get(),
                pokemon.id,
                false,
                retry as u32,
                db::now_unix(),
            )
            .await
            {
                tracing::error!("failed to record dareda result: {err}");
            }

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

    if let Err(err) = db::insert_dareda_result(
        &data.db,
        ctx.author().id.get(),
        pokemon.id,
        false,
        retry as u32,
        db::now_unix(),
    )
    .await
    {
        tracing::error!("failed to record dareda result: {err}");
    }

    Ok(())
}

/// ランキングの集計期間。
#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum RankingPeriod {
    #[name = "all"]
    All,
    #[name = "month"]
    Month,
    #[name = "week"]
    Week,
}

impl RankingPeriod {
    fn to_db_period(self) -> db::Period {
        match self {
            RankingPeriod::All => db::Period::All,
            RankingPeriod::Month => db::Period::Month,
            RankingPeriod::Week => db::Period::Week,
        }
    }

    fn label(self) -> &'static str {
        match self {
            RankingPeriod::All => "全期間",
            RankingPeriod::Month => "月間",
            RankingPeriod::Week => "週間",
        }
    }
}

/// 正解数のランキングを表示します。
#[poise::command(slash_command, guild_only)]
pub async fn ranking(
    ctx: Context<'_>,
    #[description = "集計期間（省略時は all）"] period: Option<RankingPeriod>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let period = period.unwrap_or(RankingPeriod::All);
    let rows =
        db::dareda_ranking(&ctx.data().db, period.to_db_period(), db::now_unix(), 10).await?;

    let description = if rows.is_empty() {
        "まだ記録がありません。".to_owned()
    } else {
        rows.iter()
            .enumerate()
            .map(|(i, row)| {
                format!(
                    "{}. <@{}> 正解数: {} / 平均試行: {:.1}",
                    i + 1,
                    row.user_id,
                    row.correct_count,
                    row.avg_attempts
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title(format!("だーれだ 正解数ランキング（{}）", period.label()))
                .description(description)
                .color(0xffb7c5),
        ),
    )
    .await?;

    Ok(())
}

/// だーれだの戦績を表示します。
#[poise::command(slash_command, guild_only)]
pub async fn stats(
    ctx: Context<'_>,
    #[description = "対象ユーザー（省略時は自分）"] user: Option<serenity::User>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let target = user.as_ref().unwrap_or_else(|| ctx.author());
    let stats = db::dareda_stats(&ctx.data().db, target.id.get()).await?;

    let accuracy = if stats.total_count > 0 {
        format!(
            "{:.1}%",
            stats.correct_count as f64 / stats.total_count as f64 * 100.0
        )
    } else {
        "-".to_owned()
    };
    let avg_attempts = stats
        .avg_attempts
        .map_or("-".to_owned(), |avg| format!("{avg:.1}"));
    let history = if stats.recent.is_empty() {
        "まだ記録がありません。".to_owned()
    } else {
        stats
            .recent
            .iter()
            .map(|entry| {
                format!(
                    "<t:{}:R> No.{} {}（{}回）",
                    entry.answered_at,
                    entry.pokemon_id,
                    if entry.is_correct {
                        "正解"
                    } else {
                        "不正解"
                    },
                    entry.attempts
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title(format!("だーれだ 戦績: {}", target.name))
                .color(0xffb7c5)
                .field("正解数", stats.correct_count.to_string(), true)
                .field("挑戦数", stats.total_count.to_string(), true)
                .field("正解率", accuracy, true)
                .field("平均試行回数", avg_attempts, true)
                .field("直近の履歴", history, false),
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
