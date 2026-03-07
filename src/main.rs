use axum::{
    extract::{State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::IntoResponse,
    http::{StatusCode, HeaderMap},
    routing::get,
    Router,
};
use futures_util::{stream::StreamExt, SinkExt};
use redis::{aio::ConnectionManager, Client, AsyncCommands};
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};
use tracing::{info, error, warn};

// JWT Claims структура
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,           // username
    pub user_id: i64,          // user id (может быть как i64, так и String)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub exp: i64,              // expiration timestamp
    pub iat: i64,              // issued at
}

// Глобальное состояние
#[derive(Clone)]
struct AppState {
    redis: ConnectionManager,
    jwt_secret: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    info!("Starting Presence Service...");

    // Загружаем переменные окружения
    let redis_url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| {
            warn!("JWT_SECRET not set, using default (UNSAFE for production)");
            "change_me_in_prod".to_string()
        });

    // Подключение к Redis
    let client = Client::open(redis_url.as_str())
        .expect("Failed to create Redis client");
    let redis = ConnectionManager::new(client)
        .await
        .expect("Failed to create Redis connection manager");

    let state = AppState { redis, jwt_secret };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_check))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();

    info!("Listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}

// Health check endpoint
async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

// WebSocket handler с валидацией JWT
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Извлекаем токен из заголовка Authorization
    match extract_token_from_header(&headers) {
        Ok(token) => {
            // Валидируем токен
            match validate_token(&token, &state.jwt_secret) {
                Ok(claims) => {
                    info!("User {} authenticated", claims.user_id);
                    ws.on_upgrade(move |socket| {
                        handle_socket(socket, state, claims)
                    })
                }
                Err(e) => {
                    warn!("Token validation failed: {}", e);
                    (StatusCode::UNAUTHORIZED, "Invalid token").into_response()
                }
            }
        }
        Err(_) => {
            warn!("No valid authorization header found");
            (StatusCode::UNAUTHORIZED, "Missing or invalid Authorization header").into_response()
        }
    }
}

// Извлечение токена из заголовка Authorization: Bearer <token>
fn extract_token_from_header(headers: &HeaderMap) -> Result<String, String> {
    headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|auth_header| {
            if auth_header.starts_with("Bearer ") {
                Some(auth_header[7..].to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| "Authorization header missing or invalid".to_string())
}

// Валидация JWT токена
fn validate_token(token: &str, secret: &str) -> Result<Claims, String> {
    let decoding_key = DecodingKey::from_secret(secret.as_bytes());
    let validation = Validation::new(Algorithm::HS256);

    decode::<Claims>(token, &decoding_key, &validation)
        .map(|data| data.claims)
        .map_err(|e| format!("Token validation error: {}", e))
}

// Обработка WebSocket соединения
async fn handle_socket(socket: WebSocket, state: AppState, claims: Claims) {
    let (mut sender, mut receiver) = socket.split();

    let user_id = claims.user_id;
    let redis_key = format!("presence:user:{}", user_id);
    let broadcast_channel = "presence_updates";
    let ttl_seconds: u64 = 30;

    info!("User {} connected via WebSocket", user_id);

    // Помечаем как онлайн при подключении
    let mut redis_conn = state.redis.clone();
    if let Err(e) = redis_conn.set_ex::<_, _, ()>(&redis_key, "online", ttl_seconds).await {
        error!("Failed to set initial presence: {}", e);
    } else {
        // Отправляем broadcast о новом онлайне
        let message = serde_json::json!({
            "type": "user_online",
            "user_id": user_id,
            "timestamp": chrono::Utc::now().timestamp()
        }).to_string();

        let mut pub_conn = state.redis.clone();
        if let Err(e) = pub_conn.publish::<_, _, ()>(broadcast_channel, &message).await {
            error!("Failed to broadcast user online: {}", e);
        }
    }

    // Обработка входящих сообщений
    loop {
        match receiver.next().await {
            Some(Ok(msg)) => {
                match msg {
                    Message::Text(text) => {
                        if text == "ping" {
                            // Продлеваем сессию в Redis
                            let mut redis_conn = state.redis.clone();
                            match redis_conn.set_ex::<_, _, ()>(
                                &redis_key,
                                "online",
                                ttl_seconds
                            ).await {
                                Ok(_) => {
                                    // Отвечаем понгом
                                    if let Err(e) = sender.send(Message::Text("pong".into())).await {
                                        warn!("Failed to send pong to user {}: {}", user_id, e);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to refresh presence for user {}: {}", user_id, e);
                                    break;
                                }
                            }
                        }
                    }
                    Message::Close(_) => {
                        info!("User {} requested close", user_id);
                        break;
                    }
                    Message::Ping(data) => {
                        // Автоматически отвечаем на протокольный пинг
                        if let Err(e) = sender.send(Message::Pong(data)).await {
                            warn!("Failed to send protocol pong to user {}: {}", user_id, e);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            Some(Err(e)) => {
                error!("WebSocket error for user {}: {}", user_id, e);
                break;
            }
            None => {
                info!("WebSocket closed for user {}", user_id);
                break;
            }
        }
    }

    // Удаляем из Redis при отключении
    info!("Cleaning up for user {}", user_id);
    let mut redis_conn = state.redis.clone();
    if let Err(e) = redis_conn.del::<_, ()>(&redis_key).await {
        error!("Failed to delete presence for user {}: {}", user_id, e);
    } else {
        // Отправляем broadcast о отключении
        let message = serde_json::json!({
            "type": "user_offline",
            "user_id": user_id,
            "timestamp": chrono::Utc::now().timestamp()
        }).to_string();

        let mut pub_conn = state.redis.clone();
        if let Err(e) = pub_conn.publish::<_, _, ()>(broadcast_channel, &message).await {
            error!("Failed to broadcast user offline: {}", e);
        }
    }

    info!("Socket fully closed for user {}", user_id);
}