# Usage

cargo run -- \
  --schema other_schema.json \
  --url "https://api.example.com/items" \
  --pagination-type offset \
  --data-path results \
  --total-count-path count
