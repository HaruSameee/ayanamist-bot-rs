# Ayanamistbot-rs

Ayanamistbot-tsのRustによる再実装

## Setup

### clone repo
```bash
git clone https://github.com/yourname/ayanamist-bot-rs.git
cd ayanamist-bot-rs
cp config.example.toml config.toml
cargo run
```

## TODO
- /captcha の default_member_permissions を config から注入する実装に整理
- ~~poise Command と CreateCommand の二重定義解消~~
- pokemonの実装
- ~~proxyの実装~~
    - ~~proxyコマンドの実装~~
    - ~~proxycheckコマンドの実装~~
- ayanamist.jpとの連携(内容未定)
- joinの実装