-- Migration: Add explicit transaction hashes and escrow sequence

CREATE SEQUENCE IF NOT EXISTS order_escrow_id_seq START WITH 10001;

ALTER TABLE orders
ADD COLUMN IF NOT EXISTS funding_tx_hash VARCHAR(255) NULL,
ADD COLUMN IF NOT EXISTS release_tx_hash VARCHAR(255) NULL,
ADD COLUMN IF NOT EXISTS refund_tx_hash VARCHAR(255) NULL;
