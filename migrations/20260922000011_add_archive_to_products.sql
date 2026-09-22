-- Migration 11: Add is_archived and archived_at columns to Products Table

ALTER TABLE products
    ADD COLUMN IF NOT EXISTS is_archived BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_products_is_archived ON products(is_archived);
