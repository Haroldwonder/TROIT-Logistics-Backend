-- Migration 2: Extend Products Table with Authenticity, African Made, & Warranty

ALTER TABLE products
    ADD COLUMN IF NOT EXISTS authenticity_status VARCHAR(50) NOT NULL DEFAULT 'UNVERIFIED',
    ADD COLUMN IF NOT EXISTS last_inspected_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS is_african_made BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS african_made_category VARCHAR(100) CHECK (african_made_category IS NULL OR african_made_category IN ('ELECTRONICS', 'HOME_APPLIANCES', 'FURNITURE')),
    ADD COLUMN IF NOT EXISTS warranty_months INT NOT NULL DEFAULT 0 CHECK (warranty_months >= 0),
    ADD COLUMN IF NOT EXISTS warranty_terms TEXT;

CREATE INDEX IF NOT EXISTS idx_products_is_african_made ON products(is_african_made);
CREATE INDEX IF NOT EXISTS idx_products_african_made_cat ON products(african_made_category) WHERE is_african_made = TRUE;
