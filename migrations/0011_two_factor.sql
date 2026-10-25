ALTER TABLE users
    ADD COLUMN totp_secret VARCHAR(255) NULL,
    ADD COLUMN totp_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN totp_last_step BIGINT UNSIGNED NULL;

CREATE TABLE recovery_codes (
    user_id BIGINT UNSIGNED NOT NULL,
    code_hash CHAR(64) NOT NULL,
    PRIMARY KEY (user_id, code_hash),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) CHARACTER SET utf8mb4;

-- Mot de passe validé, second facteur attendu : ce jeton n'ouvre aucune session.
CREATE TABLE login_challenges (
    token_hash CHAR(64) NOT NULL PRIMARY KEY,
    user_id BIGINT UNSIGNED NOT NULL,
    attempts INT NOT NULL DEFAULT 0,
    expires_at TIMESTAMP NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) CHARACTER SET utf8mb4;
