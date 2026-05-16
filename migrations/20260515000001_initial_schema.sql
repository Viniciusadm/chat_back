CREATE TABLE tenants (
    id BINARY(16) PRIMARY KEY,
    name TEXT NOT NULL,
    owner_user_id BINARY(16),
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE members (
    id BINARY(16) PRIMARY KEY,
    tenant_id BINARY(16) NOT NULL,
    name TEXT NOT NULL,
    role ENUM('adult', 'child') NOT NULL,
    login_code VARCHAR(255) UNIQUE,
    photo_url TEXT,
    photo_path TEXT,
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT members_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE login_codes (
    code VARCHAR(255) PRIMARY KEY,
    member_id BINARY(16) NOT NULL,
    tenant_id BINARY(16) NOT NULL,
    name_snapshot TEXT NOT NULL,
    role_snapshot ENUM('adult', 'child') NOT NULL,
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    revoked_at DATETIME(6),
    CONSTRAINT login_codes_member_fk FOREIGN KEY (member_id) REFERENCES members(id) ON DELETE CASCADE,
    CONSTRAINT login_codes_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE users (
    id BINARY(16) PRIMARY KEY,
    member_id BINARY(16) NOT NULL,
    tenant_id BINARY(16) NOT NULL,
    email VARCHAR(255) UNIQUE,
    password_hash TEXT,
    name TEXT NOT NULL,
    role ENUM('adult', 'child') NOT NULL,
    photo_url TEXT,
    photo_path TEXT,
    chat_index_built_at DATETIME(6),
    password_salt TEXT,
    password_verifier_ciphertext TEXT,
    password_verifier_iv TEXT,
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    deleted_at DATETIME(6),
    CONSTRAINT users_member_fk FOREIGN KEY (member_id) REFERENCES members(id) ON DELETE CASCADE,
    CONSTRAINT users_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

ALTER TABLE tenants
    ADD CONSTRAINT tenants_owner_user_fk
    FOREIGN KEY (owner_user_id) REFERENCES users(id);

CREATE TABLE devices (
    id BINARY(16) PRIMARY KEY,
    tenant_id BINARY(16) NOT NULL,
    user_id BINARY(16) NOT NULL,
    approved BOOLEAN NOT NULL DEFAULT FALSE,
    active BOOLEAN NOT NULL DEFAULT FALSE,
    push_token VARCHAR(255),
    public_key TEXT,
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    last_active_at DATETIME(6),
    session_at DATETIME(6),
    deactivation_reason ENUM('account_deleted', 'new_active_device', 'logout', 'admin_removed'),
    CONSTRAINT devices_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    CONSTRAINT devices_user_fk FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE refresh_tokens (
    id BINARY(16) PRIMARY KEY,
    user_id BINARY(16) NOT NULL,
    device_id BINARY(16),
    token_hash VARCHAR(255) NOT NULL UNIQUE,
    expires_at DATETIME(6) NOT NULL,
    revoked_at DATETIME(6),
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT refresh_tokens_user_fk FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    CONSTRAINT refresh_tokens_device_fk FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE chats (
    id BINARY(16) PRIMARY KEY,
    tenant_id BINARY(16) NOT NULL,
    is_group BOOLEAN NOT NULL DEFAULT FALSE,
    name TEXT NOT NULL,
    photo_url TEXT,
    photo_path TEXT,
    last_message_ciphertext TEXT,
    last_message_iv TEXT,
    last_message_type ENUM('text', 'audio', 'image'),
    last_message_at DATETIME(6),
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    CONSTRAINT chats_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE chat_participants (
    chat_id BINARY(16) NOT NULL,
    member_id BINARY(16) NOT NULL,
    PRIMARY KEY (chat_id, member_id),
    CONSTRAINT chat_participants_chat_fk FOREIGN KEY (chat_id) REFERENCES chats(id) ON DELETE CASCADE,
    CONSTRAINT chat_participants_member_fk FOREIGN KEY (member_id) REFERENCES members(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE chat_read_state (
    chat_id BINARY(16) NOT NULL,
    member_id BINARY(16) NOT NULL,
    read_up_to DATETIME(6),
    unread_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (chat_id, member_id),
    CONSTRAINT chat_read_state_chat_fk FOREIGN KEY (chat_id) REFERENCES chats(id) ON DELETE CASCADE,
    CONSTRAINT chat_read_state_member_fk FOREIGN KEY (member_id) REFERENCES members(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE messages (
    id BINARY(16) PRIMARY KEY,
    chat_id BINARY(16) NOT NULL,
    tenant_id BINARY(16) NOT NULL,
    sender_member_id BINARY(16) NOT NULL,
    type ENUM('text', 'audio', 'image') NOT NULL,
    ciphertext TEXT,
    iv TEXT,
    enc_version INTEGER,
    audio_url TEXT,
    audio_duration INTEGER,
    image_url TEXT,
    thumbnail_url TEXT,
    image_width INTEGER,
    image_height INTEGER,
    image_file_size BIGINT,
    reply_to_message_id BINARY(16),
    reply_to_sender_id BINARY(16),
    reply_to_sender_name TEXT,
    reply_to_type ENUM('text', 'audio', 'image'),
    reply_to_preview TEXT,
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    edited_at DATETIME(6),
    is_deleted BOOLEAN NOT NULL DEFAULT FALSE,
    deleted_at DATETIME(6),
    CONSTRAINT messages_chat_fk FOREIGN KEY (chat_id) REFERENCES chats(id) ON DELETE CASCADE,
    CONSTRAINT messages_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    CONSTRAINT messages_sender_fk FOREIGN KEY (sender_member_id) REFERENCES members(id) ON DELETE RESTRICT,
    CONSTRAINT messages_text_is_encrypted CHECK (
        type <> 'text'
        OR (ciphertext IS NOT NULL AND iv IS NOT NULL AND enc_version IS NOT NULL)
    )
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE message_reactions (
    message_id BINARY(16) NOT NULL,
    member_id BINARY(16) NOT NULL,
    chat_id BINARY(16) NOT NULL,
    emoji VARCHAR(64) NOT NULL,
    updated_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (message_id, member_id),
    CONSTRAINT message_reactions_message_fk FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE,
    CONSTRAINT message_reactions_member_fk FOREIGN KEY (member_id) REFERENCES members(id) ON DELETE CASCADE,
    CONSTRAINT message_reactions_chat_fk FOREIGN KEY (chat_id) REFERENCES chats(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE key_backups (
    user_id BINARY(16) NOT NULL,
    chat_id BINARY(16) NOT NULL,
    ciphertext TEXT NOT NULL,
    iv TEXT NOT NULL,
    enc_version INTEGER NOT NULL,
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (user_id, chat_id),
    CONSTRAINT key_backups_user_fk FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
    CONSTRAINT key_backups_chat_fk FOREIGN KEY (chat_id) REFERENCES chats(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE key_shares (
    device_id BINARY(16) NOT NULL,
    chat_id BINARY(16) NOT NULL,
    ephemeral_public_key TEXT NOT NULL,
    iv TEXT NOT NULL,
    ciphertext TEXT NOT NULL,
    wrapped_by_member_id BINARY(16) NOT NULL,
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (device_id, chat_id),
    CONSTRAINT key_shares_device_fk FOREIGN KEY (device_id) REFERENCES devices(id) ON DELETE CASCADE,
    CONSTRAINT key_shares_chat_fk FOREIGN KEY (chat_id) REFERENCES chats(id) ON DELETE CASCADE,
    CONSTRAINT key_shares_wrapped_by_fk FOREIGN KEY (wrapped_by_member_id) REFERENCES members(id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE media_files (
    id BINARY(16) PRIMARY KEY,
    tenant_id BINARY(16) NOT NULL,
    owner_member_id BINARY(16) NOT NULL,
    chat_id BINARY(16),
    message_id BINARY(16),
    kind VARCHAR(64) NOT NULL,
    storage_path TEXT NOT NULL,
    public_url TEXT NOT NULL,
    content_type VARCHAR(255) NOT NULL,
    size_bytes BIGINT NOT NULL,
    width INTEGER,
    height INTEGER,
    created_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    deleted_at DATETIME(6),
    CONSTRAINT media_files_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    CONSTRAINT media_files_owner_fk FOREIGN KEY (owner_member_id) REFERENCES members(id) ON DELETE RESTRICT,
    CONSTRAINT media_files_chat_fk FOREIGN KEY (chat_id) REFERENCES chats(id) ON DELETE CASCADE,
    CONSTRAINT media_files_message_fk FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE realtime_events (
    id BINARY(16) PRIMARY KEY,
    tenant_id BINARY(16) NOT NULL,
    type VARCHAR(64) NOT NULL,
    chat_id BINARY(16),
    entity_id BINARY(16),
    occurred_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    payload JSON NOT NULL,
    CONSTRAINT realtime_events_tenant_fk FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    CONSTRAINT realtime_events_chat_fk FOREIGN KEY (chat_id) REFERENCES chats(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX members_tenant_idx ON members(tenant_id);
CREATE INDEX members_tenant_role_idx ON members(tenant_id, role);
CREATE INDEX users_tenant_idx ON users(tenant_id);
CREATE INDEX users_member_idx ON users(member_id);
CREATE INDEX devices_user_idx ON devices(user_id);
CREATE INDEX devices_tenant_approved_idx ON devices(tenant_id, approved);
CREATE INDEX devices_tenant_active_idx ON devices(tenant_id, active);
CREATE INDEX devices_push_token_idx ON devices(push_token);
CREATE INDEX chats_tenant_updated_idx ON chats(tenant_id, updated_at DESC);
CREATE INDEX chat_participants_member_chat_idx ON chat_participants(member_id, chat_id);
CREATE INDEX chat_participants_chat_member_idx ON chat_participants(chat_id, member_id);
CREATE INDEX messages_chat_created_idx ON messages(chat_id, created_at DESC);
CREATE INDEX messages_sender_idx ON messages(sender_member_id);
CREATE INDEX message_reactions_chat_updated_idx ON message_reactions(chat_id, updated_at DESC);
CREATE INDEX realtime_events_tenant_occurred_idx ON realtime_events(tenant_id, occurred_at DESC);
