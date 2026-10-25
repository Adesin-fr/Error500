# error500

Alternative auto-hébergeable à Sentry, compatible avec les SDK Sentry : erreurs et stack traces (source maps), session replay (rrweb, y compris en direct), logs applicatifs, alertes par e-mail. Une seule application (backend Rust, interface Vue servie par le même binaire), multi-utilisateurs et multi-projets.

## Déploiement

L'image est publiée sur `ghcr.io/adesin-fr/error500` à chaque tag du dépôt (`latest` suit la dernière version stable). Il faut une base **MariaDB ou MySQL** ; les tables sont créées et mises à jour au démarrage.

**Avec Docker Compose** (application + MariaDB), à partir de [`docker-compose.yml`](docker-compose.yml) :

```sh
echo 'PUBLIC_URL=https://errors.example.com' >> .env
echo 'DB_PASSWORD=un-mot-de-passe-solide'    >> .env
docker compose up -d
```

Ouvrez ensuite `PUBLIC_URL` : la page `/setup` crée le premier compte, qui est administrateur. Créez un projet, copiez son **DSN** (Réglages du projet) dans la configuration du SDK Sentry de votre application.

**Avec `docker run`** et une base existante :

```sh
docker run -d --name error500 -p 8080:8080 -v error500-data:/data \
  -e DATABASE_URL='mysql://utilisateur:motdepasse@hote:3306/error500' \
  -e PUBLIC_URL='https://errors.example.com' \
  ghcr.io/adesin-fr/error500:latest
```

La base (`error500` ici) doit exister. Le conteneur écoute sur le port 8080 et expose `/health`. Placez-le derrière un reverse proxy HTTPS (Caddy, nginx, Traefik…) qui transmet les requêtes telles quelles ; la taille maximale des requêtes doit être au moins `MAX_BODY_MB`.

### Variables d'environnement

Elles se passent à l'environnement du conteneur, ou dans un fichier `.env` (voir [`.env.example`](.env.example)) ; l'environnement réel prime sur le fichier.

| Variable | Défaut | Rôle |
|---|---|---|
| `DATABASE_URL` | — (**obligatoire**) | Base MySQL/MariaDB : `mysql://utilisateur:motdepasse@hote:3306/base` |
| `PUBLIC_URL` | `http://localhost:8080` | Adresse publique, sans barre finale. Sert aux DSN, aux liens des e-mails et aux cookies (en `https`, ils passent en `Secure`) |
| `BIND` | `0.0.0.0:8080` (image) | Adresse d'écoute |
| `MAX_BODY_MB` | `20` | Taille maximale d'une requête d'ingestion, en Mo, après décompression |
| `STORAGE` | `local` | Stockage des replays et source maps : `local` ou `s3` |
| `STORAGE_PATH` | `/data/blobs` (image) | Dossier du stockage local (monter un volume sur `/data`) |
| `S3_BUCKET`, `S3_ACCESS_KEY`, `S3_SECRET_KEY` | — | Obligatoires si `STORAGE=s3` |
| `S3_REGION` | `us-east-1` | Région S3 |
| `S3_ENDPOINT` | — | Pour un S3 compatible (MinIO, Scaleway, OVH…) |
| `SMTP_URL` | — | Active les alertes e-mail : `smtps://utilisateur:motdepasse@smtp.example.com` (ou `smtp://hote:587?tls=required`) |
| `MAIL_FROM` | `error500@localhost` | Expéditeur des alertes |
| `ALERT_MAX_PER_HOUR` | `10` | Alertes e-mail maximum par projet et par heure (`0` = illimité) |
| `SECRET_KEY` | — | Clé AES-256 (64 caractères hex, `openssl rand -hex 32`) qui chiffre les secrets 2FA. Sans elle, la double authentification est indisponible. À conserver : la perdre rend les secrets illisibles (`error500 disable-2fa <email>` débloque un compte) |
| `RUST_LOG` | `info` | Niveau des journaux (`error`, `warn`, `info`, `debug`) |
| `ENV_FILE` | `.env` | Chemin d'un autre fichier de variables |

### Ce qu'il faut savoir

- **Données à sauvegarder :** la base MariaDB, et le dossier `/data` (replays et source maps) si le stockage est local.
- **Mise à jour :** `docker compose pull && docker compose up -d`. Les migrations s'appliquent seules.
- **Rétention :** les logs et les replays plus vieux que 30 jours sont supprimés (réglable par projet dans les Réglages, `0` = conserver). Les erreurs ne sont jamais purgées.
- **Alertes e-mail :** pour une nouvelle issue ou une régression, aux membres du projet qui ne les ont pas coupées. Sans `SMTP_URL`, elles sont désactivées.
- **Source maps :** compatibles avec `sentry-cli sourcemaps upload` (`SENTRY_URL` = votre `PUBLIC_URL`, `SENTRY_AUTH_TOKEN` = le jeton d'upload affiché dans les Réglages du projet), ou avec `curl`.
- **Instance unique :** l'application ne gère pas de verrou entre plusieurs instances ; l'ingestion et la lecture fonctionnent, mais la purge de rétention tourne dans chacune.

## Développement

Voir [`CLAUDE.md`](CLAUDE.md) : architecture, organisation du code, tests (MariaDB en RAM, TDD) et commandes de développement.
