CREATE TABLE IF NOT EXISTS "urls" (
    "id" SERIAL PRIMARY KEY,
    "unique_key" VARCHAR(50) NOT NULL,
    "full_url" TEXT NOT NULL,
    "created_at" TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS unique_key_index ON urls (unique_key);

COMMENT ON COLUMN urls.unique_key IS 'uuid or nano id etc';
COMMENT ON COLUMN urls.full_url IS 'Full url';
