# Presences Service

WebSocket-based real-time user presence tracking service for the Sup platform. This service handles online/offline status of users using Redis as a persistent store.

## Features

- 🔐 **JWT Authentication**: Validates JWT tokens from Authorization header (HS256)
- 🔄 **Real-time Presence**: Tracks user online/offline status with automatic cleanup
- 📡 **WebSocket Support**: Efficient async WebSocket implementation using Axum
- 🗄️ **Redis Integration**: Uses Redis with TTL for automatic presence expiration
- 📢 **Broadcast Events**: Publishes presence changes to Redis pub/sub for other services
- ✅ **Health Check**: Built-in health check endpoint for orchestration

## Technology Stack

- **Language**: Rust (Edition 2024)
- **Web Framework**: Axum 0.7 with WebSocket support
- **Async Runtime**: Tokio 1.x
- **Storage**: Redis 7.2+
- **Authentication**: jsonwebtoken 9 (HS256)
- **Logging**: tracing 0.1

## Prerequisites

- Rust 1.70+ (for Edition 2024)
- Redis 7.2+
- Docker & Docker Compose (for containerized deployment)

## Environment Variables

```bash
# Redis connection URL
REDIS_URL=redis://127.0.0.1:6379

# JWT secret key (must match user-service JWT_SECRET)
JWT_SECRET=your-secret-key-here

# Logging level
RUST_LOG=info
```

## Building

### Local Development

```bash
# Check compilation
cargo check

# Build debug version
cargo build

# Build release version
cargo build --release
```

### Docker

```bash
# Build Docker image
docker build -t presences-service:latest .

# Run with docker-compose
docker-compose up presences-service
```

## API Endpoints

### WebSocket

**Endpoint**: `GET /ws`

**Authentication**: Required JWT token in Authorization header

**Headers**:
```
Authorization: Bearer <jwt-token>
```

**JWT Token Structure**:
```json
{
  "sub": "username",
  "user_id": 123,
  "avatar_url": "https://example.com/avatar.jpg",
  "iat": 1708000000,
  "exp": 1708015000
}
```

**Connection Flow**:
1. Client connects to WebSocket with valid JWT in Authorization header
2. Server validates token signature and expiration
3. If valid, user is marked as "online" in Redis with 30-second TTL
4. Server broadcasts "user_online" event to `presence_updates` channel
5. Client should periodically send "ping" message to refresh TTL

**Client Messages**:
- `ping`: Refresh presence TTL and receive "pong" response

**Server Messages**:
- `pong`: Response to client ping

**Protocol Messages**:
- WebSocket Ping/Pong handled automatically

**Disconnection**:
- On close: User marked as offline, presence data deleted from Redis
- Broadcast: "user_offline" event published

### Health Check

**Endpoint**: `GET /health`

**Response**: 200 OK

## Usage Example

### JavaScript/TypeScript Client

```typescript
// Generate JWT token (from auth service)
const token = await getJWTToken();

// Connect to WebSocket
const ws = new WebSocket('ws://localhost:3000/ws', [], {
  headers: {
    Authorization: `Bearer ${token}`
  }
});

ws.onopen = () => {
  console.log('Connected to presence service');

  // Send periodic ping to keep alive
  setInterval(() => {
    ws.send('ping');
  }, 10000);
};

ws.onmessage = (event) => {
  if (event.data === 'pong') {
    console.log('Presence refreshed');
  }
};

ws.onerror = (error) => {
  console.error('WebSocket error:', error);
};

ws.onclose = () => {
  console.log('Disconnected from presence service');
};
```

### cURL Test (requires JWT token)

```bash
# Get JWT token first (from user-service)
TOKEN=$(curl -X POST http://localhost:8081/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"testuser","password":"password"}' \
  | jq -r '.accessToken')

# Connect to WebSocket (requires wscat or similar tool)
wscat -c "ws://localhost:3000/ws" \
  --header "Authorization: Bearer $TOKEN"

# In wscat shell, send:
# ping
# Should receive: pong
```

## Data Format

### Redis Keys

```
presence:user:{user_id} = "online"  (with 30s TTL)
```

### Broadcast Messages

**Format**: JSON published to `presence_updates` Redis channel

**User Online**:
```json
{
  "type": "user_online",
  "user_id": 123,
  "timestamp": 1708000000
}
```

**User Offline**:
```json
{
  "type": "user_offline",
  "user_id": 123,
  "timestamp": 1708000015
}
```

### Subscribe to Presence Updates

```bash
redis-cli
> SUBSCRIBE presence_updates

# Will receive messages like:
# {"type": "user_online", "user_id": 123, "timestamp": 1708000000}
```

## Running with Docker Compose

Complete stack with all services:

```bash
# Start all services
docker-compose up -d

# View logs
docker-compose logs -f presences-service

# Stop all services
docker-compose down

# Stop and clean volumes
docker-compose down -v
```

**Ports**:
- Presences Service: `3000`
- User Service: `8081`
- API Gateway: `8080`
- Redis: `6379`
- Redis Commander UI: `6060` (accessible at http://localhost:6060)
- Postgres: `5432`

## Testing

### Manual Testing Steps

1. **Start Services**:
```bash
docker-compose up -d redis presences-service user-service
```

2. **Create User & Get Token** (example):
```bash
curl -X POST http://localhost:8081/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "username": "testuser",
    "email": "test@example.com",
    "password": "password123"
  }'

# Login to get token
curl -X POST http://localhost:8081/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "username": "testuser",
    "password": "password123"
  }'
```

3. **Monitor Redis**:
```bash
# Open Redis Commander at http://localhost:6060
# Or use redis-cli:
redis-cli -h localhost
> KEYS presence:user:*
> GET presence:user:123
> SUBSCRIBE presence_updates
```

4. **Connect WebSocket**:
```bash
# Install wscat: npm install -g wscat
wscat -c "ws://localhost:3000/ws" \
  --header "Authorization: Bearer YOUR_JWT_TOKEN"

# Send ping message
ping

# Should receive pong response
```

## Architecture

```
WebSocket Client
    ↓
Presences Service
    ├─ JWT Validation (Authorization header)
    ├─ Redis Set (presence:user:{id}: "online", TTL: 30s)
    └─ Redis Pub/Sub (presence_updates channel)
       ├─ user_online event
       └─ user_offline event
```

## Error Handling

| Status | Response | Cause |
|--------|----------|-------|
| 401 | Unauthorized | Missing Authorization header |
| 401 | Invalid token | Expired or tampered JWT token |
| 200 | WebSocket Upgrade | Valid token, connection established |

## Performance Considerations

- **Connection TTL**: 30 seconds (configurable in code)
- **Redis Operations**: All async/non-blocking using Tokio
- **Memory Usage**: O(n) where n = number of online users
- **Horizontal Scaling**: Share Redis instance across multiple instances

## Logging

Structured logging with tracing:

```
time=... level=INFO service=presence-service user=123 msg="User 123 connected via WebSocket"
time=... level=INFO service=presence-service user=123 msg="User 123 requested close"
time=... level=ERROR service=presence-service user=123 msg="Failed to set initial presence"
```

## Security

- ✅ JWT token validation required for all connections
- ✅ HS256 signature verification
- ✅ Token expiration check
- ✅ No sensitive data in logs
- ⚠️ Ensure JWT_SECRET matches across all services
- ⚠️ Use HTTPS/WSS in production

## Troubleshooting

### Connection Refused
```
Error: Failed to connect to presences-service
Solution: Ensure REDIS_URL is correct and Redis is running
```

### Token Validation Failed
```
401 Unauthorized: Token validation error
Solution: Verify JWT_SECRET matches user-service value
```

### No Broadcast Messages
```
Messages not appearing in presence_updates channel
Solution: Verify Redis pub/sub is enabled, check REDIS_URL
```

### High Memory Usage
```
Presence data growing unbounded
Solution: Verify TTL is set correctly (should be 30s)
            Check that clients send periodic ping messages
```

## Development

### Code Structure

- `src/main.rs`:
  - `Claims`: JWT claims struct
  - `AppState`: Application state with Redis connection
  - `main()`: Server setup and startup
  - `health_check()`: Health endpoint
  - `ws_handler()`: WebSocket upgrade with JWT validation
  - `extract_token_from_header()`: Parse Authorization header
  - `validate_token()`: JWT signature and expiration check
  - `handle_socket()`: Main WebSocket message handling and Redis operations

### Future Enhancements

- [ ] Horizontal scaling with message deduplication
- [ ] Presence status levels (online, away, dnd, invisible)
- [ ] User activity tracking
- [ ] Presence history/analytics
- [ ] GraphQL subscription support
- [ ] Metrics & monitoring endpoints

## License

Part of the Sup platform

## Support
No support for this module yet

For issues and questions, contact the backend team or check the Sup platform documentation.
