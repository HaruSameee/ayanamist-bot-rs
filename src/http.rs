use reqwest::Client;
use std::sync::LazyLock;

pub static CLIENT: LazyLock<Client> = LazyLock::new(Client::new);
