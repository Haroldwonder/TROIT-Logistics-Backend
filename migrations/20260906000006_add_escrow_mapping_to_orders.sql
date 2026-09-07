-- Migration: Add escrow mapping and blockchain transaction hash columns to orders table

ALTER TABLE orders
ADD COLUMN IF NOT EXISTS escrow_id BIGINT UNIQUE NULL,
ADD COLUMN IF NOT EXISTS blockchain_tx_hash VARCHAR(255) NULL;

CREATE INDEX IF NOT EXISTS idx_orders_escrow_id ON orders(escrow_id);
