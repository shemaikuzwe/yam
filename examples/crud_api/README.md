# crud_api
Simple CRUD API in Rust + Diesel + Postgres + JWT auth. Same API implemented with three different HTTP frameworks (one per branch).


## Stack

- **Runtime:** Tokio
- **DB:** Postgres + Diesel ORM
- **Auth:** bcrypt + JWT (jsonwebtoken with `rust_crypto`)
- **Logging:** tracing / tracing-subscriber

## Requirements

- Rust (edition 2024)
- Postgres
- Diesel CLI: `cargo install diesel_cli --no-default-features --features postgres`

## Setup

```bash
git clone https://github.com/shemaikuzwe/crud_api.git
cd crud_api

# env
cp .env.example .env  
```

```bash
# run migrations
diesel migration run

cargo run
# -> http://localhost:3000
```

## Project Structure

```
src/
  main.rs        # router + server bootstrap
  lib.rs         # env / db connection
  admin/         # admin CRUD (controller, service, dtos)
  auth/          # signup / login
  middleware/    # auth + logging
  models.rs
  schema.rs
migrations/
```

## License

MIT
