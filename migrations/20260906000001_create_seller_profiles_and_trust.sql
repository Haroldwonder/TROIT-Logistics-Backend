-- Migration 1: Seller Profiles and Trust History Tables

-- Seller Profiles Table
CREATE TABLE IF NOT EXISTS seller_profiles (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    store_name VARCHAR(255),
    store_address TEXT,
    trust_level VARCHAR(10) NOT NULL DEFAULT 'LV1' CHECK (trust_level IN ('LV1', 'LV2', 'LV3', 'LV4', 'LV5')),
    seller_grade VARCHAR(10) NOT NULL DEFAULT 'Grade C' CHECK (seller_grade IN ('Grade C', 'Grade B', 'Grade A')),
    successful_transactions INT NOT NULL DEFAULT 0 CHECK (successful_transactions >= 0),
    fulfillment_rate DOUBLE PRECISION NOT NULL DEFAULT 100.0 CHECK (fulfillment_rate >= 0.0 AND fulfillment_rate <= 100.0),
    verification_status VARCHAR(50) NOT NULL DEFAULT 'PENDING',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_seller_profiles_user_id ON seller_profiles(user_id);
CREATE INDEX IF NOT EXISTS idx_seller_profiles_trust_level ON seller_profiles(trust_level);
CREATE INDEX IF NOT EXISTS idx_seller_profiles_seller_grade ON seller_profiles(seller_grade);

-- Seller Trust History Table (Append-Only Audit Log)
CREATE TABLE IF NOT EXISTS seller_trust_history (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    seller_id UUID NOT NULL REFERENCES seller_profiles(id) ON DELETE CASCADE,
    old_level VARCHAR(10) NOT NULL,
    new_level VARCHAR(10) NOT NULL,
    reason TEXT NOT NULL,
    trigger_transaction_id UUID REFERENCES orders(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_seller_trust_history_seller_id ON seller_trust_history(seller_id);
