-- Migration 12: Admin Bootstrap State Table
-- Tracks one-time initialization state for server-side admin account provisioning

CREATE TABLE IF NOT EXISTS admin_bootstrap_state (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    bootstrap_name VARCHAR(100) NOT NULL UNIQUE,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_admin_bootstrap_state_name ON admin_bootstrap_state(bootstrap_name);
