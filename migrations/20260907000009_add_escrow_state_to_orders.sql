-- Migration: Add escrow_state column to orders table for Soroban escrow lifecycle tracking

ALTER TABLE orders
ADD COLUMN IF NOT EXISTS escrow_state VARCHAR(50) DEFAULT 'NONE' NOT NULL;

CREATE INDEX IF NOT EXISTS idx_orders_escrow_state ON orders (escrow_state);
