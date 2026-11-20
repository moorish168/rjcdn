-- Customer DNS provider accounts (GoEdge-style) for CNAME-mode ACME DNS-01.
CREATE TABLE IF NOT EXISTS dns_providers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    provider_type TEXT NOT NULL,
    -- JSON credentials sealed with the same AES-GCM scheme as SSH grants.
    credentials_enc TEXT NOT NULL DEFAULT '',
    note TEXT NOT NULL DEFAULT '',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT dns_providers_type_check CHECK (
        provider_type IN ('cloudflare', 'alidns', 'dnspod')
    )
);

CREATE INDEX IF NOT EXISTS idx_dns_providers_type ON dns_providers (provider_type);

ALTER TABLE domains
    ADD COLUMN IF NOT EXISTS dns_provider_id UUID REFERENCES dns_providers(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_domains_dns_provider_id
    ON domains (dns_provider_id)
    WHERE dns_provider_id IS NOT NULL;
