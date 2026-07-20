use crate::{Error, http};
use lru::LruCache;
use rand::Rng;
use serde::Deserialize;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell, RwLock};

const DEFAULT_BASE_URL: &str = "https://pokeapi.co/api/v2";
const IMAGE_CACHE_CAPACITY: usize = 50;

#[derive(Debug, Deserialize)]
struct PokemonJson {
    sprites: Sprites,
}

#[derive(Debug, Deserialize)]
struct Sprites {
    front_default: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SpeciesJson {
    names: Vec<NameEntry>,
    flavor_text_entries: Vec<FlavorTextEntry>,
}

#[derive(Debug, Deserialize)]
struct NameEntry {
    name: String,
    language: Language,
}

#[derive(Debug, Deserialize)]
struct FlavorTextEntry {
    flavor_text: String,
    language: Language,
}

#[derive(Debug, Deserialize)]
struct Language {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ResourceList {
    count: i64,
}

struct Inner {
    pokemon_cache: RwLock<HashMap<i16, Arc<PokemonJson>>>,
    species_cache: RwLock<HashMap<i16, Arc<SpeciesJson>>>,
    image_bytes_cache: Mutex<LruCache<i16, Arc<Vec<u8>>>>,
    total: OnceCell<i16>,
}

/// PokeAPI の非同期クライアント。ベース URL を差し替えられるためテストでモックサーバーを注入できる。
#[derive(Clone)]
pub struct PokemonApi {
    base_url: String,
    inner: Arc<Inner>,
}

impl PokemonApi {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            inner: Arc::new(Inner {
                pokemon_cache: RwLock::new(HashMap::new()),
                species_cache: RwLock::new(HashMap::new()),
                image_bytes_cache: Mutex::new(LruCache::new(
                    NonZeroUsize::new(IMAGE_CACHE_CAPACITY).unwrap_or(NonZeroUsize::MIN),
                )),
                total: OnceCell::new(),
            }),
        }
    }

    async fn pokemon(&self, id: i16) -> Result<Arc<PokemonJson>, Error> {
        if let Some(cached) = self.inner.pokemon_cache.read().await.get(&id) {
            return Ok(Arc::clone(cached));
        }

        let url = format!("{}/pokemon/{id}", self.base_url);
        let fetched = Arc::new(
            http::CLIENT
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .json::<PokemonJson>()
                .await?,
        );

        self.inner
            .pokemon_cache
            .write()
            .await
            .insert(id, Arc::clone(&fetched));

        Ok(fetched)
    }

    async fn species(&self, id: i16) -> Result<Arc<SpeciesJson>, Error> {
        if let Some(cached) = self.inner.species_cache.read().await.get(&id) {
            return Ok(Arc::clone(cached));
        }

        let url = format!("{}/pokemon-species/{id}", self.base_url);
        let fetched = Arc::new(
            http::CLIENT
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .json::<SpeciesJson>()
                .await?,
        );

        self.inner
            .species_cache
            .write()
            .await
            .insert(id, Arc::clone(&fetched));

        Ok(fetched)
    }

    async fn total(&self) -> Result<i16, Error> {
        let total = self
            .inner
            .total
            .get_or_try_init(|| async {
                let url = format!("{}/pokemon-species?offset=0&limit=1", self.base_url);
                let list = http::CLIENT
                    .get(url)
                    .send()
                    .await?
                    .error_for_status()?
                    .json::<ResourceList>()
                    .await?;
                i16::try_from(list.count).map_err(Error::from)
            })
            .await?;

        Ok(*total)
    }

    pub async fn random<R>(&self, rng: &mut R) -> Result<Pokemon, Error>
    where
        R: Rng,
    {
        let total = self.total().await?;
        Ok(Pokemon {
            id: pick_id(total, rng),
            api: self.clone(),
        })
    }
}

impl Default for PokemonApi {
    fn default() -> Self {
        Self::new(DEFAULT_BASE_URL)
    }
}

/// 抽選対象のポケモン ID を返す。PokeAPI の ID は 1 始まりで total を含む。
fn pick_id<R: Rng>(total: i16, rng: &mut R) -> i16 {
    rng.gen_range(1..=total)
}

#[derive(Clone)]
pub struct Pokemon {
    pub id: i16,
    api: PokemonApi,
}

impl Pokemon {
    #[cfg(test)]
    fn new(id: i16, api: PokemonApi) -> Self {
        Self { id, api }
    }

    pub async fn name(&self) -> Result<Option<String>, Error> {
        Ok(self
            .api
            .species(self.id)
            .await?
            .names
            .iter()
            .find(|n| n.language.name == "ja-hrkt")
            .map(|n| n.name.clone()))
    }

    pub async fn flavor_text(&self) -> Result<Option<String>, Error> {
        Ok(self
            .api
            .species(self.id)
            .await?
            .flavor_text_entries
            .iter()
            .find(|f| f.language.name == "ja-hrkt")
            .map(|f| f.flavor_text.clone()))
    }

    pub async fn image_url(&self) -> Result<Option<String>, Error> {
        Ok(self
            .api
            .pokemon(self.id)
            .await?
            .sprites
            .front_default
            .clone())
    }

    pub async fn image_bytes(&self) -> Result<Option<Arc<Vec<u8>>>, Error> {
        let cached = {
            let mut cache = self.api.inner.image_bytes_cache.lock().await;
            cache.get(&self.id).cloned()
        };

        if let Some(bytes) = cached {
            return Ok(Some(bytes));
        }

        let Some(image_url) = self.image_url().await? else {
            return Ok(None);
        };

        let bytes = Arc::new(
            http::CLIENT
                .get(image_url)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?
                .to_vec(),
        );

        let mut cache = self.api.inner.image_bytes_cache.lock().await;
        cache.put(self.id, Arc::clone(&bytes));

        Ok(Some(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn species_body() -> serde_json::Value {
        serde_json::json!({
            "names": [
                { "name": "フシギダネ", "language": { "name": "ja-hrkt" } },
                { "name": "Bulbasaur", "language": { "name": "en" } }
            ],
            "flavor_text_entries": [
                { "flavor_text": "うまれたときから せなかに", "language": { "name": "ja-hrkt" } },
                { "flavor_text": "A strange seed", "language": { "name": "en" } }
            ]
        })
    }

    #[tokio::test]
    async fn total_fetches_count_once() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pokemon-species"))
            .and(query_param("offset", "0"))
            .and(query_param("limit", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "count": 1025
            })))
            .mount(&server)
            .await;

        let api = PokemonApi::new(server.uri());
        assert_eq!(api.total().await.unwrap(), 1025);
        // 2回目は OnceCell から返り、追加リクエストが発生しない
        assert_eq!(api.total().await.unwrap(), 1025);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn random_returns_pokemon_with_id_in_range() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pokemon-species"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "count": 1025
            })))
            .mount(&server)
            .await;

        let api = PokemonApi::new(server.uri());
        let mut rng = StdRng::seed_from_u64(42);
        let pokemon = api.random(&mut rng).await.unwrap();
        assert!((1..=1025).contains(&pokemon.id));
    }

    #[tokio::test]
    async fn name_selects_ja_hrkt() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pokemon-species/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(species_body()))
            .mount(&server)
            .await;

        let api = PokemonApi::new(server.uri());
        let pokemon = Pokemon::new(1, api);
        assert_eq!(pokemon.name().await.unwrap().as_deref(), Some("フシギダネ"));
    }

    #[tokio::test]
    async fn flavor_text_selects_ja_hrkt() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pokemon-species/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(species_body()))
            .mount(&server)
            .await;

        let api = PokemonApi::new(server.uri());
        let pokemon = Pokemon::new(1, api);
        assert_eq!(
            pokemon.flavor_text().await.unwrap().as_deref(),
            Some("うまれたときから せなかに")
        );
    }

    #[tokio::test]
    async fn image_url_reads_front_default() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pokemon/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sprites": { "front_default": "https://example.invalid/1.png" }
            })))
            .mount(&server)
            .await;

        let api = PokemonApi::new(server.uri());
        let pokemon = Pokemon::new(1, api);
        assert_eq!(
            pokemon.image_url().await.unwrap().as_deref(),
            Some("https://example.invalid/1.png")
        );
    }

    #[tokio::test]
    async fn not_found_returns_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pokemon-species/9999"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let api = PokemonApi::new(server.uri());
        let pokemon = Pokemon::new(9999, api);
        assert!(pokemon.name().await.is_err());
    }

    #[tokio::test]
    async fn image_bytes_is_cached_after_first_fetch() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/pokemon/1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "sprites": { "front_default": format!("{}/sprites/1.png", server.uri()) }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/sprites/1.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"png-bytes".to_vec()))
            .mount(&server)
            .await;

        let api = PokemonApi::new(server.uri());
        let pokemon = Pokemon::new(1, api);

        let first = pokemon.image_bytes().await.unwrap().unwrap();
        let second = pokemon.image_bytes().await.unwrap().unwrap();
        assert_eq!(first.as_slice(), b"png-bytes");
        assert!(Arc::ptr_eq(&first, &second));

        let sprite_requests = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/sprites/1.png")
            .count();
        assert_eq!(sprite_requests, 1);
    }

    #[test]
    fn pick_id_stays_within_one_to_total() {
        let mut rng = StdRng::seed_from_u64(7);
        for _ in 0..1000 {
            let id = pick_id(1025, &mut rng);
            assert!((1..=1025).contains(&id));
        }
    }
}
