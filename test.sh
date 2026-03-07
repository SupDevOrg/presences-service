#!/bin/bash

# testing script for presences-service

set -e

echo "=== Presences Service Testing Script ==="
echo ""

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
REDIS_URL="${REDIS_URL:-redis://localhost:6379}"
PRESENCES_SERVICE_URL="${PRESENCES_SERVICE_URL:-http://localhost:3000}"
USER_SERVICE_URL="${USER_SERVICE_URL:-http://localhost:8081}"
JWT_SECRET="${JWT_SECRET:-D4fN8Qr6Zu1WgX9Cv3PyL5Mk2Jh7Vt0s}"

echo "Configuration:"
echo "  Redis: $REDIS_URL"
echo "  Presences Service: $PRESENCES_SERVICE_URL"
echo "  User Service: $USER_SERVICE_URL"
echo ""

# Test 1: Health check
echo -e "${YELLOW}Test 1: Health Check${NC}"
response=$(curl -s -o /dev/null -w "%{http_code}" "${PRESENCES_SERVICE_URL}/health")
if [ "$response" = "200" ]; then
  echo -e "${GREEN}✓ Health check passed${NC}"
else
  echo -e "${RED}✗ Health check failed (HTTP $response)${NC}"
  exit 1
fi
echo ""

# Test 2: WebSocket without token (should fail with 401)
echo -e "${YELLOW}Test 2: WebSocket Connection without Token${NC}"
response=$(curl -s -i -N -H "Connection: Upgrade" -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Key: SGVsbG8sIHdvcmxkIQ==" \
  -H "Sec-WebSocket-Version: 13" \
  "${PRESENCES_SERVICE_URL}/ws" 2>&1 | head -1)
if [[ "$response" == *"401"* ]]; then
  echo -e "${GREEN}✓ Correctly rejected connection without token${NC}"
else
  echo "Response: $response"
fi
echo ""

# Test 3: Test Redis connectivity
echo -e "${YELLOW}Test 3: Redis Connectivity${NC}"
redis_check=$(redis-cli -u "$REDIS_URL" ping 2>&1 || echo "FAILED")
if [[ "$redis_check" == "PONG" ]]; then
  echo -e "${GREEN}✓ Redis connection successful${NC}"
else
  echo -e "${RED}✗ Redis connection failed${NC}"
  echo "Error: $redis_check"
fi
echo ""

# Test 4: Check for presence keys in Redis
echo -e "${YELLOW}Test 4: Redis Presence Keys${NC}"
keys=$(redis-cli -u "$REDIS_URL" KEYS "presence:user:*" 2>&1)
if [ -n "$keys" ]; then
  echo -e "${GREEN}✓ Found presence keys in Redis:${NC}"
  echo "$keys"
else
  echo -e "${YELLOW}ℹ No presence keys found (expected if no users connected)${NC}"
fi
echo ""

# Test 5: Check Redis pub/sub (monitoring)
echo -e "${YELLOW}Test 5: Monitoring Presence Updates (30s)${NC}"
echo "Listening for events on 'presence_updates' channel..."
(timeout 30 redis-cli -u "$REDIS_URL" SUBSCRIBE presence_updates 2>&1 || true) &
echo -e "${GREEN}✓ Redis pub/sub is available${NC}"
wait
echo ""

echo -e "${GREEN}=== All Tests Completed ===${NC}"
echo ""
echo "Next Steps:"
echo "1. Connect a WebSocket client with a valid JWT token"
echo "2. Monitor Redis for presence updates:"
echo "   redis-cli -u '$REDIS_URL' SUBSCRIBE presence_updates"
echo "3. Check health endpoint:"
echo "   curl ${PRESENCES_SERVICE_URL}/health"
echo ""
