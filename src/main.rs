use clap::Parser;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the schema JSON file
    #[arg(short, long)]
    schema: String,

    /// Base URL for the API
    #[arg(short, long)]
    url: String,

    /// API Key (optional)
    #[arg(short, long)]
    api_key: Option<String>,

    /// Header key-value pairs in format "key=value"
    #[arg(short, long, value_parser = parse_key_val)]
    headers: Vec<(String, String)>,

    /// Output directory
    #[arg(short, long, default_value = "output")]
    output_dir: String,

    /// Page size
    #[arg(short, long, default_value_t = 250)]
    page_size: i32,

    /// Rate limit delay in milliseconds
    #[arg(short, long, default_value_t = 100)]
    rate_limit: u64,

    /// Pagination type (offset, cursor, page)
    #[arg(long, default_value = "page")]
    pagination_type: String,

    /// Response path to data array (e.g., "data" or "results")
    #[arg(long, default_value = "data")]
    data_path: String,

    /// Response path to total count (e.g., "totalCount" or "count")
    #[arg(long, default_value = "totalCount")]
    total_count_path: String,
}

#[derive(Debug)]
struct ApiSchema {
    endpoint_config: EndpointConfig,
    response_mapping: HashMap<String, String>,
}

#[derive(Debug)]
struct EndpointConfig {
    base_url: String,
    headers: HashMap<String, String>,
    pagination: PaginationConfig,
    rate_limit: Duration,
}

#[derive(Debug)]
struct PaginationConfig {
    pagination_type: PaginationType,
    page_size: i32,
    data_path: String,
    total_count_path: String,
}

#[derive(Debug)]
enum PaginationType {
    Offset,
    Cursor,
    Page,
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{}`", s))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

#[derive(Debug)]
struct Scraper {
    client: reqwest::Client,
    config: EndpointConfig,
    output_dir: String,
}

impl Scraper {
    async fn new(args: Args) -> Result<Self, Box<dyn std::error::Error>> {
        let schema = load_schema(&args.schema)?;

        let headers = args.headers.into_iter().collect();

        let pagination_type = match args.pagination_type.to_lowercase().as_str() {
            "offset" => PaginationType::Offset,
            "cursor" => PaginationType::Cursor,
            _ => PaginationType::Page,
        };

        let config = EndpointConfig {
            base_url: args.url,
            headers,
            pagination: PaginationConfig {
                pagination_type,
                page_size: args.page_size,
                data_path: args.data_path,
                total_count_path: args.total_count_path,
            },
            rate_limit: Duration::from_millis(args.rate_limit),
        };

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            client,
            config,
            output_dir: args.output_dir,
        })
    }

    async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !Path::new(&self.output_dir).exists() {
            fs::create_dir(&self.output_dir)?;
        }

        let initial_response = self.fetch_page(1).await?;
        let total_count = self.get_total_count(&initial_response)?;
        let total_pages =
            (total_count as f32 / self.config.pagination.page_size as f32).ceil() as i32;

        println!("Total items: {}", total_count);
        println!("Total pages: {}", total_pages);

        for page in 1..=total_pages {
            println!("Fetching page {} of {}", page, total_pages);
            tokio::time::sleep(self.config.rate_limit).await;

            match self.fetch_page(page).await {
                Ok(response) => {
                    self.save_page_to_file(page, &response)?;
                }
                Err(e) => {
                    eprintln!("Error fetching page {}: {}", page, e);
                }
            }
        }

        println!("Download complete! Files saved in '{}'", self.output_dir);
        Ok(())
    }

    async fn fetch_page(&self, page: i32) -> Result<Value, Box<dyn std::error::Error>> {
        let mut request = self.client.get(&self.config.base_url);

        // Add headers
        for (key, value) in &self.config.headers {
            request = request.header(key, value);
        }

        // Add pagination parameters based on type
        let params = match self.config.pagination.pagination_type {
            PaginationType::Offset => {
                let offset = (page - 1) * self.config.pagination.page_size;
                vec![
                    ("offset", offset.to_string()),
                    ("limit", self.config.pagination.page_size.to_string()),
                ]
            }
            PaginationType::Cursor => {
                // Implement cursor-based pagination
                vec![
                    ("cursor", page.to_string()),
                    ("limit", self.config.pagination.page_size.to_string()),
                ]
            }
            PaginationType::Page => {
                vec![
                    ("page", page.to_string()),
                    ("pageSize", self.config.pagination.page_size.to_string()),
                ]
            }
        };

        let response = request.query(&params).send().await?.json().await?;
        Ok(response)
    }

    fn get_total_count(&self, response: &Value) -> Result<i32, Box<dyn std::error::Error>> {
        response
            .pointer(&format!("/{}", self.config.pagination.total_count_path))
            .and_then(|v| v.as_i64())
            .map(|v| v as i32)
            .ok_or_else(|| "Could not find total count in response".into())
    }

    fn save_page_to_file(
        &self,
        page: i32,
        response: &Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let data = response
            .pointer(&format!("/{}", self.config.pagination.data_path))
            .ok_or("Could not find data in response")?;

        let filename = format!("{}/page_{}.json", self.output_dir, page);
        let mut file = File::create(&filename)?;
        let json = serde_json::to_string_pretty(data)?;
        file.write_all(json.as_bytes())?;
        println!("Saved page {} to {}", page, filename);
        Ok(())
    }
}

fn load_schema(path: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(path)?;
    let schema: Value = serde_json::from_str(&contents)?;
    Ok(schema)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let scraper = Scraper::new(args).await?;
    scraper.run().await?;
    Ok(())
}
