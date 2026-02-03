<<<<<<< HEAD
use crate::{Context, Data, Error};
use ::serenity::all::{CreateActionRow, CreateButton, CreateEmbed};
use poise::serenity_prelude as serenity;
use rand::seq::SliceRandom;
use rand::thread_rng;
use regex::Regex;
use reqwest::multipart;
use serde::Deserialize;

const PROXYSCRAPE_GET_PROXY_ENDPOINT: &str =
    "https://api.proxyscrape.com/?request=displayproxies&proxytype=all&timeout=1500";

const PROXYSCRAPE_CHECK_PROXY_ENDPOINT: &str = "https://api.proxyscrape.com/v2/online_check.php";

#[derive(Deserialize)]
#[serde(untagged)]
enum ProxyCheckResultType {
    Str(String),
    Bool(bool),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ProxyCheckResultCountry {
    Str(String),
    Bool(bool),
}

#[derive(Deserialize)]
struct ProxyCheckResult {
    working: bool,
    r#type: ProxyCheckResultType,
    ip: String,
    port: String,
    country: ProxyCheckResultCountry,
    #[allow(dead_code)]
    ind: String,
}

#[derive(Deserialize)]
struct ProxyCheckResults(Vec<ProxyCheckResult>);

struct Proxy {
    ip: String,
    port: String,
}

async fn get_proxies() -> reqwest::Result<Vec<Proxy>> {
    Ok(reqwest::get(PROXYSCRAPE_GET_PROXY_ENDPOINT)
        .await?
        .text()
        .await?
        .lines()
        .filter_map(|l| {
            l.split_once(":").and_then(|p| {
                if !(p.0.is_empty() || p.0.is_empty()) {
                    Some(p)
                } else {
                    None
                }
            })
        })
        .map(|(ip, port)| Proxy {
            ip: ip.to_string(),
            port: port.to_string(),
        })
        .collect())
}

async fn check_proxies(proxies: &[Proxy]) -> Result<ProxyCheckResults, Error> {
    // TODO: Clientは毎回作るべきではない
    let client = reqwest::Client::new();
    let mut form = multipart::Form::new();

    for (i, proxy) in proxies.iter().enumerate() {
        form = form.text("ip_addr[]", format!("{}:{}-{}", proxy.ip, proxy.port, i));
    }

    Ok(client
        .post(PROXYSCRAPE_CHECK_PROXY_ENDPOINT)
        .multipart(form)
        .send()
        .await?
        .json()
        .await?)
}

/// チェックを行い結果を表示します
#[poise::command(slash_command, guild_only, rename = "proxycheck")]
pub async fn proxycheck_command(
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
    let Some(result) = results.0.first() else {
        ctx.reply("プロキシのチェックに失敗しました").await?;

        return Ok(());
    };

    let typ = match &result.r#type {
        ProxyCheckResultType::Str(s) => Some(s.clone()),
        ProxyCheckResultType::Bool(_) => None,
    };
    let country = match &result.country {
        ProxyCheckResultCountry::Str(s) => Some(s.clone()),
        ProxyCheckResultCountry::Bool(_) => None,
    };
    let embed = CreateEmbed::new()
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
#[poise::command(slash_command, guild_only, rename = "proxy")]
pub async fn proxy_command(
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
    let working_results = results.0.iter().filter(|r| r.working);

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
                        ProxyCheckResultType::Str(s) => {
                            if s.is_empty() {
                                None
                            } else {
                                Some(s.clone())
                            }
                        }
                        ProxyCheckResultType::Bool(_) => None,
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
    let row = CreateActionRow::Buttons(vec![button]);

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .components(vec![row]),
    )
    .await?;

    Ok(())
}

struct ProxyInfo {
    address: String,
    typ: String,
}

const BUTTON_DEFINES: &[(&str, &str)] = &[
    ("all", "All"),
    ("http", "HTTP(s)"),
    ("socks4", "Socks4"),
    ("socks5", "Socks5"),
    ("scheme", "All (+Scheme)"),
];

pub async fn handle_component(
    ctx: &serenity::Context,
    _data: &Data,
    i: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let id = i.data.custom_id.as_str();

    match id {
        "proxy:download_start" => {
            // TODO: 正規表現は毎回作るべきではない
            let re = Regex::new(r"\s*\|\s*").unwrap();
            let Some(desc) = i
                .message
                .embeds
                .first()
                .and_then(|e| e.description.as_ref())
            else {
                i.create_response(
                    ctx,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("このダウンロードに関連付けられたプロキシが取得できません")
                            .ephemeral(true),
                    ),
                )
                .await?;

                return Ok(());
            };
            let proxies: Vec<ProxyInfo> = desc
                .lines()
                .filter_map(|l| {
                    let parts: Vec<&str> = re.split(l).collect();
                    let (Some(address), Some(typ)) = (if parts.len() > 2 {
                        (parts.first(), parts.get(2))
                    } else {
                        (parts.first(), parts.get(1))
                    }) else {
                        return None;
                    };

                    Some(ProxyInfo {
                        address: address.to_string(),
                        typ: typ.to_string(),
                    })
                })
                .collect();

            if proxies.is_empty() {
                i.create_response(
                    ctx,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("このダウンロードに関連付けられたプロキシが取得できません")
                            .ephemeral(true),
                    ),
                )
                .await?;

                return Ok(());
            }

            let buttons: Vec<CreateButton> = BUTTON_DEFINES
                .iter()
                .map(|(id, label)| {
                    CreateButton::new(format!("proxy:download:{}", id))
                        .label(label.to_string())
                        .style(serenity::ButtonStyle::Secondary)
                })
                .collect();
            let row = CreateActionRow::Buttons(buttons);

            i.create_response(
                ctx,
                serenity::CreateInteractionResponse::Message(
                    serenity::CreateInteractionResponseMessage::new()
                        .content("Choose Download Type")
                        .ephemeral(true)
                        .components(vec![row]),
                ),
            )
            .await?;

            Ok(())
        }
        id if id.starts_with("proxy:download:") => {
            // TODO: 正規表現は毎回作るべきではない
            let re = Regex::new(r"\s*\|\s*").unwrap();
            let Some(typ) = id.strip_prefix("proxy:download:") else {
                i.create_response(
                    ctx,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("不明なプロキシの種類です")
                            .ephemeral(true),
                    ),
                )
                .await?;

                return Ok(());
            };
            let Some(message_id) = i
                .message
                .message_reference
                .as_ref()
                .and_then(|m| m.message_id.as_ref())
            else {
                i.create_response(
                    ctx,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("このダウンロードに関連付けられたプロキシが取得できません")
                            .ephemeral(true),
                    ),
                )
                .await?;

                return Ok(());
            };
            let message = i
                .channel_id
                .message(&ctx, message_id)
                // TODO
                .await?;
            let desc = message.embeds.first().and_then(|e| e.description.as_ref());

            let Some(proxies) = desc.map(|desc| {
                desc.lines().filter_map(|l| {
                    let parts: Vec<&str> = re.split(l).collect();
                    let (Some(address), Some(typ)) = (if parts.len() > 2 {
                        (parts.first(), parts.get(2))
                    } else {
                        (parts.first(), parts.get(1))
                    }) else {
                        return None;
                    };

                    Some(ProxyInfo {
                        address: address.to_string(),
                        typ: typ.to_string(),
                    })
                })
            }) else {
                i.create_response(
                    ctx,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("このダウンロードに関連付けられたプロキシが取得できません")
                            .ephemeral(true),
                    ),
                )
                .await?;

                return Ok(());
            };

            let download_proxies: Vec<String> = match typ {
                "http" | "socks4" | "socks5" => proxies
                    .filter(|p| p.typ == typ)
                    .map(|p| p.address)
                    .collect(),
                "scheme" => proxies
                    .map(|p| format!("{}://{}", p.typ, p.address))
                    .collect(),
                _ => proxies.map(|p| p.address).collect(),
            };
            let attachment = serenity::CreateAttachment::bytes(
                download_proxies.join("\n").as_bytes(),
                // TODO: ファイル名はランダムかタイムスタンプを含む
                "proxies.txt",
            );

            i.create_response(
                ctx,
                serenity::CreateInteractionResponse::Message(
                    serenity::CreateInteractionResponseMessage::new()
                        .content("Complete")
                        .add_file(attachment)
                        .ephemeral(true),
                ),
            )
            .await?;

            Ok(())
        }
        _ => Ok(()),
    }
}
=======
use crate::{Context, Data, Error};
use ::serenity::all::{CreateActionRow, CreateButton, CreateEmbed};
use poise::serenity_prelude as serenity;
use rand::seq::SliceRandom;
use rand::thread_rng;
use regex::Regex;
use reqwest::multipart;
use serde::Deserialize;

const PROXYSCRAPE_GET_PROXY_ENDPOINT: &str =
    "https://api.proxyscrape.com/?request=displayproxies&proxytype=all&timeout=1500";

const PROXYSCRAPE_CHECK_PROXY_ENDPOINT: &str = "https://api.proxyscrape.com/v2/online_check.php";

#[derive(Deserialize)]
#[serde(untagged)]
enum ProxyCheckResultType {
    Str(String),
    Bool(bool),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ProxyCheckResultCountry {
    Str(String),
    Bool(bool),
}

#[derive(Deserialize)]
struct ProxyCheckResult {
    working: bool,
    r#type: ProxyCheckResultType,
    ip: String,
    port: String,
    country: ProxyCheckResultCountry,
    #[allow(dead_code)]
    ind: String,
}

#[derive(Deserialize)]
struct ProxyCheckResults(Vec<ProxyCheckResult>);

struct Proxy {
    ip: String,
    port: String,
}

async fn get_proxies() -> reqwest::Result<Vec<Proxy>> {
    Ok(reqwest::get(PROXYSCRAPE_GET_PROXY_ENDPOINT)
        .await?
        .text()
        .await?
        .lines()
        .filter_map(|l| {
            l.split_once(":").and_then(|p| {
                if !(p.0.is_empty() || p.0.is_empty()) {
                    Some(p)
                } else {
                    None
                }
            })
        })
        .map(|(ip, port)| Proxy {
            ip: ip.to_string(),
            port: port.to_string(),
        })
        .collect())
}

async fn check_proxies(proxies: &[Proxy]) -> Result<ProxyCheckResults, Error> {
    // TODO: Clientは毎回作るべきではない
    let client = reqwest::Client::new();
    let mut form = multipart::Form::new();

    for (i, proxy) in proxies.iter().enumerate() {
        form = form.text("ip_addr[]", format!("{}:{}-{}", proxy.ip, proxy.port, i));
    }

    Ok(client
        .post(PROXYSCRAPE_CHECK_PROXY_ENDPOINT)
        .multipart(form)
        .send()
        .await?
        .json()
        .await?)
}

/// チェックを行い結果を表示します
#[poise::command(slash_command, guild_only, rename = "proxycheck")]
pub async fn proxycheck_command(
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
    let Some(result) = results.0.first() else {
        ctx.reply("プロキシのチェックに失敗しました").await?;

        return Ok(());
    };

    let typ = match &result.r#type {
        ProxyCheckResultType::Str(s) => Some(s.clone()),
        ProxyCheckResultType::Bool(_) => None,
    };
    let country = match &result.country {
        ProxyCheckResultCountry::Str(s) => Some(s.clone()),
        ProxyCheckResultCountry::Bool(_) => None,
    };
    let embed = CreateEmbed::new()
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
#[poise::command(slash_command, guild_only, rename = "proxy")]
pub async fn proxy_command(
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
    let working_results = results.0.iter().filter(|r| r.working);

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
                        ProxyCheckResultType::Str(s) => {
                            if s.is_empty() {
                                None
                            } else {
                                Some(s.clone())
                            }
                        }
                        ProxyCheckResultType::Bool(_) => None,
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
    let row = CreateActionRow::Buttons(vec![button]);

    ctx.send(
        poise::CreateReply::default()
            .embed(embed)
            .components(vec![row]),
    )
    .await?;

    Ok(())
}

struct ProxyInfo {
    address: String,
    typ: String,
}

const BUTTON_DEFINES: &[(&str, &str)] = &[
    ("all", "All"),
    ("http", "HTTP(s)"),
    ("socks4", "Socks4"),
    ("socks5", "Socks5"),
    ("scheme", "All (+Scheme)"),
];

pub async fn handle_component(
    ctx: &serenity::Context,
    _data: &Data,
    i: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let id = i.data.custom_id.as_str();

    match id {
        "proxy:download_start" => {
            // TODO: 正規表現は毎回作るべきではない
            let re = Regex::new(r"\s*\|\s*").unwrap();
            let Some(desc) = i
                .message
                .embeds
                .first()
                .and_then(|e| e.description.as_ref())
            else {
                i.create_response(
                    ctx,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("このダウンロードに関連付けられたプロキシが取得できません")
                            .ephemeral(true),
                    ),
                )
                .await?;

                return Ok(());
            };
            let proxies: Vec<ProxyInfo> = desc
                .lines()
                .filter_map(|l| {
                    let parts: Vec<&str> = re.split(l).collect();
                    let (Some(address), Some(typ)) = (if parts.len() > 2 {
                        (parts.first(), parts.get(2))
                    } else {
                        (parts.first(), parts.get(1))
                    }) else {
                        return None;
                    };

                    Some(ProxyInfo {
                        address: address.to_string(),
                        typ: typ.to_string(),
                    })
                })
                .collect();

            if proxies.is_empty() {
                i.create_response(
                    ctx,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("このダウンロードに関連付けられたプロキシが取得できません")
                            .ephemeral(true),
                    ),
                )
                .await?;

                return Ok(());
            }

            let buttons: Vec<CreateButton> = BUTTON_DEFINES
                .iter()
                .map(|(id, label)| {
                    CreateButton::new(format!("proxy:download:{}", id))
                        .label(label.to_string())
                        .style(serenity::ButtonStyle::Secondary)
                })
                .collect();
            let row = CreateActionRow::Buttons(buttons);

            i.create_response(
                ctx,
                serenity::CreateInteractionResponse::Message(
                    serenity::CreateInteractionResponseMessage::new()
                        .content("Choose Download Type")
                        .ephemeral(true)
                        .components(vec![row]),
                ),
            )
            .await?;

            Ok(())
        }
        id if id.starts_with("proxy:download:") => {
            // TODO: 正規表現は毎回作るべきではない
            let re = Regex::new(r"\s*\|\s*").unwrap();
            let Some(typ) = id.strip_prefix("proxy:download:") else {
                i.create_response(
                    ctx,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("不明なプロキシの種類です")
                            .ephemeral(true),
                    ),
                )
                .await?;

                return Ok(());
            };
            let Some(message_id) = i
                .message
                .message_reference
                .as_ref()
                .and_then(|m| m.message_id.as_ref())
            else {
                i.create_response(
                    ctx,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("このダウンロードに関連付けられたプロキシが取得できません")
                            .ephemeral(true),
                    ),
                )
                .await?;

                return Ok(());
            };
            let message = i
                .channel_id
                .message(&ctx, message_id)
                // TODO
                .await?;
            let desc = message.embeds.first().and_then(|e| e.description.as_ref());

            let Some(proxies) = desc.map(|desc| {
                desc.lines().filter_map(|l| {
                    let parts: Vec<&str> = re.split(l).collect();
                    let (Some(address), Some(typ)) = (if parts.len() > 2 {
                        (parts.first(), parts.get(2))
                    } else {
                        (parts.first(), parts.get(1))
                    }) else {
                        return None;
                    };

                    Some(ProxyInfo {
                        address: address.to_string(),
                        typ: typ.to_string(),
                    })
                })
            }) else {
                i.create_response(
                    ctx,
                    serenity::CreateInteractionResponse::Message(
                        serenity::CreateInteractionResponseMessage::new()
                            .content("このダウンロードに関連付けられたプロキシが取得できません")
                            .ephemeral(true),
                    ),
                )
                .await?;

                return Ok(());
            };

            let download_proxies: Vec<String> = match typ {
                "http" | "socks4" | "socks5" => proxies
                    .filter(|p| p.typ == typ)
                    .map(|p| p.address)
                    .collect(),
                "scheme" => proxies
                    .map(|p| format!("{}://{}", p.typ, p.address))
                    .collect(),
                _ => proxies.map(|p| p.address).collect(),
            };
            let attachment = serenity::CreateAttachment::bytes(
                download_proxies.join("\n").as_bytes(),
                // TODO: ファイル名はランダムかタイムスタンプを含む
                "proxies.txt",
            );

            i.create_response(
                ctx,
                serenity::CreateInteractionResponse::Message(
                    serenity::CreateInteractionResponseMessage::new()
                        .content("Complete")
                        .add_file(attachment)
                        .ephemeral(true),
                ),
            )
            .await?;

            Ok(())
        }
        _ => Ok(()),
    }
}
>>>>>>> be6c202 (Interaction fix)
