# Examples

# For Pokemon TCG API
cargo run -- \
  --schema pokemon_schema.json \
  --url "https://api.pokemontcg.io/v2/cards" \
  --api-key "your_api_key" \
  --headers "X-Api-Key=your_api_key" \
  --pagination-type page \
  --data-path data \
  --total-count-path totalCount

# For a different API with offset pagination
cargo run -- \
  --schema other_schema.json \
  --url "https://api.example.com/items" \
  --pagination-type offset \
  --data-path results \
  --total-count-path count
