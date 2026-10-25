# error500

Alternative auto-hébergeable à Sentry, compatible avec les SDK Sentry.

## Architecture

- **Une seule application** : backend Rust (axum) + frontend Vue 3/TypeScript (Vite) dans le même dépôt et le même binaire.
- Liaison Rust ↔ Vue via **Inertia** (crate `inertia_rs`, feature `axum` — https://github.com/stuarth/inertia-rs ; `inertia-rust` écarté car limité à actix-web). Pas d'API JSON séparée ni de seconde app pour l'UI.
- Base de données : MySQL/MariaDB (sqlx).
- Stockage des blobs (replays rrweb, source maps) : local ou S3, derrière un trait.
- Ingestion : API compatible SDK Sentry (`/api/{project_id}/envelope/`, `/store/`, auth par DSN).

Fonctions : multi-utilisateur, multi-projets, ingestion d'erreurs, stack traces + code map (source maps), session replay (rrweb), logs applicatifs.

## TDD obligatoire

Pour chaque nouvelle fonctionnalité :

1. Écrire un test rouge.
2. Le lancer et **constater qu'il échoue** pour la bonne raison (une erreur de compilation sur un symbole absent compte).
3. Écrire le code minimal pour le faire passer.
4. Refactorer, tests verts.

Aucun code de production sans test rouge préalable.

## Tests

- Les tests d'intégration jouent sur une **base MariaDB démarrée en RAM** (datadir sur un RAM disk macOS), jamais sur une base persistante.
- MariaDB est le binaire fourni par DBngin (`/Users/Shared/DBngin/mariadb/11.8.8_arm`, surchargeable via `MARIADB_BASEDIR`). L'instance de test est **séparée** de celles de DBngin : port 3307, socket et datadir dans `/Volumes/error500-testdb`.
- Démarrer : `scripts/test-db.sh start` (crée le RAM disk 512 Mo, initialise la base, crée `error500_test`). État : `status`. Arrêter et libérer la RAM : `scripts/test-db.sh stop`.
- Variable utilisée par les tests : `DATABASE_URL=mysql://root@127.0.0.1:3307/error500_test` (root sans mot de passe, local uniquement).
- Lancer : `cargo test`
- Les tests purs (parseurs, domaine) ne nécessitent pas de base.

## Organisation du code (`src/`)

- `envelope` : parseur du format d'enveloppe Sentry.
- `auth` : extraction de la clé publique (`X-Sentry-Auth`, `?sentry_key=`, DSN de l'enveloppe), construction du DSN.
- `users` : utilisateurs (argon2), sessions (jeton hashé SHA-256). Le premier utilisateur créé est admin.
- `projects` : projets, membres (`owner`/`member`), clé publique, rotation. Un admin voit tous les projets.
- `event` : normalisation d'un événement et empreinte de regroupement en issues.
- `ingest` : envelope → issues/events, logs, replays (segments rrweb décompressés, stockés dans le `BlobStore`).
- `storage` : `BlobStore` (disque local, S3 ou mémoire) via `object_store`.
- `http` : routes `/api/{id}/envelope/`, `/api/{id}/store/`, `/health` ; gzip/deflate, CORS, limite de taille.
- `config` : variables d'environnement (`DATABASE_URL`, `BIND`, `PUBLIC_URL`, `MAX_BODY_MB`, `STORAGE`, `STORAGE_PATH`, `S3_*`).
- Migrations SQL dans `migrations/` (une par fonctionnalité, appliquées au démarrage).

## Lancer

```
scripts/test-db.sh start   # ou toute autre base MariaDB/MySQL
DATABASE_URL=mysql://root@127.0.0.1:3307/error500_test cargo run -- create-user me@example.com Moi "mot de passe"
DATABASE_URL=... cargo run -- create-project me@example.com "Mon app"   # affiche le DSN
DATABASE_URL=... cargo run                                               # serveur sur BIND (0.0.0.0:8080)
```

## Reste à faire

Interface Inertia/Vue (connexion, projets, issues, stack traces, replay), source maps (code map), consultation des logs, gestion des projets/membres via l'UI, test réel contre un S3 (seul le local et la construction S3 sont testés).
