-- Migration 3: Detailed Product & Order Inspection Reports

CREATE TABLE IF NOT EXISTS inspection_reports (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    order_id UUID REFERENCES orders(id) ON DELETE SET NULL,
    inspector_id UUID REFERENCES users(id) ON DELETE SET NULL,
    authenticity_verified BOOLEAN NOT NULL DEFAULT FALSE,
    physical_condition VARCHAR(255) NOT NULL,
    serial_number VARCHAR(255),
    functional_tests JSONB,
    photos_json JSONB,
    notes TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_inspection_reports_product_id ON inspection_reports(product_id);
CREATE INDEX IF NOT EXISTS idx_inspection_reports_order_id ON inspection_reports(order_id);
CREATE INDEX IF NOT EXISTS idx_inspection_reports_inspector_id ON inspection_reports(inspector_id);
