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

## Organisation du code

Backend (`src/`) :

- `envelope` : parseur du format d'enveloppe Sentry.
- `auth` : extraction de la clé publique (`X-Sentry-Auth`, `?sentry_key=`, DSN de l'enveloppe), construction du DSN.
- `users` : utilisateurs (argon2), sessions (jeton hashé SHA-256). Le premier utilisateur créé est admin.
- `projects` : projets, membres (`owner`/`member`), clé publique, jeton d'upload, rotation. Un admin voit tous les projets.
- `event` : normalisation d'un événement, empreinte de regroupement en issues, contexte d'affichage (liste blanche de champs).
- `ingest` : envelope → issues/events, logs, replays (segments rrweb décompressés : JSON, zlib, gzip ou deflate brut ; stockés dans le `BlobStore`).
- `artifact_bundle` : lecture de l'archive zip de sentry-cli (`manifest.json`, source maps, `debug-id`).
- `cli_upload` : protocole de `sentry-cli sourcemaps upload` (`/api/0/organizations/{org}/chunk-upload/`, `artifactbundle/assemble/`, releases). Le projet est celui du jeton d'upload ; le slug d'organisation est ignoré. Les chunks (`chunks/{projet}/{sha1}`) ne sont pas purgés.
- `sourcemaps` : stockage par projet, association par `debug_id` ou release + chemin (`~/static/app.js.map`), résolution des frames.
- `stacktrace` : vue exceptions/frames (le plus récent d'abord). Le nom d'une fonction vient du token de **l'appelant** (le nom d'une map à une position est celui de la fonction appelée).
- `queries` : lectures pour l'UI (issues, événements, logs, replays, erreurs liées).
- `storage` : `BlobStore` (disque local, S3 ou mémoire) via `object_store`.
- `http` : API d'ingestion `/api/{id}/envelope/`, `/store/`, `/sourcemaps/` (Bearer), `/health` ; gzip/deflate, CORS, limite de taille.
- `web` : pages Inertia (`inertia_rs`) : `auth`, `projects`, `issues`, `replays`, `logs`, `settings`, `admin`, assets embarqués. 404 (pas 403) sur un projet inaccessible.
- `config` : variables d'environnement (`DATABASE_URL`, `BIND`, `PUBLIC_URL`, `MAX_BODY_MB`, `STORAGE`, `STORAGE_PATH`, `S3_*`, `VITE_DEV_URL`).
- `lib.rs::app` : routeur unique (ingestion + web). Migrations SQL dans `migrations/` (appliquées au démarrage).

Frontend (`frontend/`, Vue 3 + TypeScript + Vite, `<script setup>`) :

- `src/pages/**` : une page par composant Inertia rendu par Rust (`Issues/Show`, `Replays/Show`…).
- `src/components/**` : `StackTrace`, `StackFrame`, `Breadcrumbs`, `ReplayPlayer` (`@sentry-internal/rrweb-player`, chargé à la demande), `Pagination`…
- `src/types.ts` : formes des props envoyées par Rust. `src/lib/format.ts` : formatage.
- Tests : Vitest + Vue Test Utils (`npm test`), tests boîte noire ; `src/test/inertiaMock.ts` remplace `@inertiajs/vue3`.
- Le build (`frontend/dist`) est embarqué dans le binaire (rust-embed) ; `build.rs` relance la compilation quand il change.

## Lancer

```
scripts/test-db.sh start   # ou toute autre base MariaDB/MySQL
export DATABASE_URL=mysql://root@127.0.0.1:3307/error500_test PUBLIC_URL=http://localhost:8080

(cd frontend && npm install && npm run build)   # une fois, puis à chaque changement du frontend
cargo run -- create-user me@example.com Moi "mot de passe"   # ou via /setup au premier lancement
cargo run -- create-project me@example.com "Mon app"          # affiche le DSN
cargo run                                                     # serveur sur BIND (0.0.0.0:8080)
```

Développement du frontend avec rechargement à chaud : `cd frontend && npm run dev`, puis `VITE_DEV_URL=http://localhost:5173 cargo run`.
Tests : `cargo test` (base en RAM démarrée) et `cd frontend && npm test`. `npm run build` lance aussi `vue-tsc` (TypeScript 5 : `vue-tsc` ne supporte pas encore TS 7).

## Source maps

Avec **sentry-cli** (testé avec la 3.8.2), jeton d'upload affiché dans Réglages :

```
export SENTRY_URL=https://error500.example.com SENTRY_AUTH_TOKEN=<jeton d'upload> SENTRY_ORG=x SENTRY_PROJECT=x
sentry-cli sourcemaps inject ./dist
sentry-cli sourcemaps upload --release 1.0.0 ./dist
```

Ou avec `curl` : `POST /api/{id}/sourcemaps/?release=1.0.0&name=~%2Fstatic%2Fapp.min.js.map` avec `Authorization: Bearer <jeton d'upload>` et le fichier `.map` en corps. Association : `debug_id` (`debug_meta` de l'événement ↔ `debugId` de la map ou en-tête `debug-id` du manifeste) en priorité, sinon release + chemin.

## Reste à faire

Test réel contre un S3 (seul le local et la construction S3 sont testés) ; `sentry-cli` : seul l'upload par archive (`artifact_bundles`) est géré, pas les anciens `releases files` ni les fichiers de debug ; replays mobiles (`replay_video`) ; alertes/notifications ; suppression de projets ; rétention des données.
