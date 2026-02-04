use crate::proxy::api::{OptString, Proxy, check_proxies, get_proxies};
use crate::{Context, Error};
use poise::serenity_prelude as serenity;
use rand::seq::SliceRandom;
use rand::thread_rng;

/// チェックを行い結果を表示します
#[poise::command(slash_command, guild_only)]
pub async fn proxycheck(
    ctx: Context<'_>,
    #[description = "チェックしたいプロキシ。ip:portの形式で入力"] proxy: String,
) -> Result<(), Error> {
    let Some((ip, port)) = proxy.split_once(":") else {
        ctx.reply("ip:portの形式で記述してください").await?;

        return Ok(());
    };

    ctx.defer().await?;

    let results = match check_proxies(&[Proxy {
        ip: ip.to_string(),
        port: port.to_string(),
    }])
    .await
    {
        Ok(result) => result,
        Err(err) => {
            tracing::error!("{err:?}");
            ctx.reply("プロキシのチェックに失敗しました").await?;

            return Ok(());
        }
    };
    let Some(result) = results.first() else {
        ctx.reply("プロキシのチェックに失敗しました").await?;

        return Ok(());
    };

    let typ = match &result.r#type {
        OptString::Str(s) => Some(s.clone()),
        OptString::Bool(_) => None,
    };
    let country = match &result.country {
        OptString::Str(s) => Some(s.clone()),
        OptString::Bool(_) => None,
    };
    let embed = serenity::CreateEmbed::new()
        .color(if result.working {
            serenity::Color::DARK_GREEN
        } else {
            serenity::Color::RED
        })
        .title("Proxy Checker")
        .field(
            "Status",
            if result.working {
                "Working"
            } else {
                "Not Working"
            },
            false,
        )
        .field("Type", typ.unwrap_or("Unknown".to_owned()), true)
        .field(
            "Country",
            country.map_or("Unknown".to_owned(), |s| {
                format!(":flag_{}:", s.to_lowercase())
            }),
            true,
        );

    ctx.send(poise::CreateReply::default().embed(embed)).await?;

    Ok(())
}

/// ランダムにプロキシを取得して、チェックを行い結果を表示します
#[poise::command(slash_command, guild_only)]
pub async fn proxy(
    ctx: Context<'_>,
    #[description = "取得する個数（1以上50以下）"]
    #[min = 1]
    #[max = 50]
    amount: Option<usize>,
) -> Result<(), Error> {
    let amount = amount.unwrap_or(1);

    if !(1..=50).contains(&amount) {
        ctx.reply("取得する個数は1以上50以下である必要があります")
            .await?;

        return Ok(());
    }

    ctx.defer().await?;

    let mut proxies = get_proxies().await?;

    proxies.shuffle(&mut thread_rng());

    let selected_proxy = &proxies[0..proxies.len().min(amount)];
    let results = match check_proxies(selected_proxy).await {
        Ok(results) => results,
        Err(err) => {
            // TODO
            tracing::warn!("{err:?}");

            ctx.reply("プロキシのチェックに失敗しました").await?;

            return Ok(());
        }
    };
    let working_results = results.iter().filter(|r| r.working);

    let button = serenity::CreateButton::new("proxy:download_start")
        .style(serenity::ButtonStyle::Secondary)
        .label("Download");
    let embed = serenity::CreateEmbed::new()
        .color(serenity::Color::DARK_GREEN)
        .title("Proxy Scraper")
        .description(
            working_results
                .map(|r| {
                    let typ: Option<String> = match &r.r#type {
                        OptString::Str(s) => {
                            if s.is_empty() {
                                None
                            } else {
                                Some(s.clone())
                            }
                        }
                        OptString::Bool(_) => None,
                    };

                    format!(
                        "{}:{} | {}",
                        r.ip,
                        r.port,
                        typ.unwrap_or("Unknown".to_owned())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
    let row = serenity::CreateActionRow::Buttons(vec![button]);

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .components(vec![row]),
    )
    .await?;

    Ok(())
}
