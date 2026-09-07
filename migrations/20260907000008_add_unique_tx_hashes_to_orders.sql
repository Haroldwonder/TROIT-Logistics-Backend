-- Add UNIQUE partial indexes for order transaction hashes
CREATE UNIQUE INDEX IF NOT EXISTS idx_orders_funding_tx_hash ON orders (funding_tx_hash) WHERE funding_tx_hash IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_orders_release_tx_hash ON orders (release_tx_hash) WHERE release_tx_hash IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_orders_refund_tx_hash ON orders (refund_tx_hash) WHERE refund_tx_hash IS NOT NULL;
