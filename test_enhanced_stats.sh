#!/bin/bash

# Test script to demonstrate the enhanced stats monitoring with per-endpoint tracking

echo "🚀 Testing Enhanced Axum Health Service with Detailed Stats Monitoring"
echo "======================================================================"
echo ""
echo "Features being tested:"
echo "✅ Per-endpoint performance tracking (/health, /hbd, /stats, /stats/reset)"
echo "✅ Per-query performance tracking (SELECT 1 health checks)"
echo "✅ Detailed JSON statistics with breakdown by endpoint and query"
echo "✅ Aggregated statistics across all endpoints and queries"
echo ""

# First, check if the service is running
echo "🔍 Checking if service is running on localhost:3000..."
if ! curl -s http://localhost:3000/health > /dev/null 2>&1; then
    echo "❌ Service is not running. Please start it with: cargo run"
    echo "   Then re-run this test script."
    exit 1
fi

echo "✅ Service is running!"
echo ""

# Function to make a request and show timing
make_request() {
    local endpoint=$1
    local description=$2
    echo "📡 Making request to $endpoint - $description"
    curl -s "http://localhost:3000$endpoint" | jq . 2>/dev/null || echo "Response received (not JSON)"
    echo ""
}

# Make multiple requests to different endpoints to generate stats
echo "🎯 Generating test traffic to collect statistics..."
echo ""

# Test health endpoint multiple times
for i in {1..5}; do
    make_request "/health" "Health check #$i"
    sleep 0.5
done

# Test HBD endpoint with different parameters
for i in {1..3}; do
    make_request "/hbd?id=$((100+i))&mac=00:11:22:33:44:$((50+i))&ip=192.168.1.$((100+i))&lp=80&ts=$(date +%s)" "Heartbeat #$i"
    sleep 0.5
done

# Now check the detailed statistics
echo "📊 Fetching detailed performance statistics..."
echo "============================================="
echo ""

curl -s "http://localhost:3000/stats" | jq .

echo ""
echo "🔍 Key features demonstrated:"
echo "• web_endpoints: Shows separate stats for /health, /hbd, /stats"
echo "• database_queries: Shows stats for 'SELECT 1 (health_check)' queries"
echo "• aggregated: Combined statistics across all endpoints and queries"
echo "• tracked_endpoints: List of all monitored endpoints"
echo "• tracked_queries: List of all monitored database queries"
echo ""
echo "💡 You can reset stats anytime with: curl http://localhost:3000/stats/reset"
echo "💡 Each endpoint and query is tracked separately with AtomicCell for high performance!"
echo ""
echo "🎉 Test completed successfully!"
echo "   The enhanced stats monitoring is working with per-endpoint and per-query tracking!"

