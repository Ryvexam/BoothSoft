# Envoi des photos en ligne (galerie du site de l'exploitant)

Statut : **fait** (lot U1, 18/11/2026). Décision du propriétaire du 18/11/2026 : exception
assumée à « pas de cloud / pas de partage » (`docs/ROADMAP.md` § 7), dans les limites
ci-dessous.

## Principe

Après l'événement, l'exploitant envoie le ZIP de l'événement (le même que « Exporter (ZIP) »)
à **son propre site**, qui publie une galerie privée pour son client (lien + QR code) et
lui envoie le lien. BoothSoft ne connaît aucun service particulier : il parle un petit
protocole HTTP (ci-dessous) que le site de l'exploitant implémente.

## Limites (non négociables)

- **Désactivé par défaut** : rien ne part tant qu'aucune adresse de site n'est réglée.
- **Toujours à la main**, dans la maintenance (Données & export). Jamais pendant un événement,
  jamais automatiquement, jamais côté invités. Le QR code ne s'affiche que dans la maintenance.
- **Seule sortie réseau** de BoothSoft ; aucune télémétrie, aucun port ouvert.
- HTTPS obligatoire (HTTP accepté seulement vers `localhost` / `127.0.0.1`, pour les essais).
- Tout le travail (ZIP, sha256, HTTP) sur le fil `bs-upload`, jamais sur le fil d'interface.

## Confiance : deux preuves

1. **Clé de la borne** `bsk_…`, créée sur le site, saisie une fois en maintenance. Stockée dans
   `BoothSoft/upload-key` (droits 0600 sous Unix), jamais réaffichée (seulement `bsk_…abcd`).
2. **Code à 6 chiffres** de l'application d'authentification du gérant du site, tapé sur la
   borne (pavé tactile) à chaque envoi. Un code ne sert qu'une fois ; le site bloque après 5 codes faux.

Chaque morceau porte en plus le secret de l'envoi (`uploadToken`).

## Parcours

Maintenance → Données & export :

1. Carte **Envoi des photos en ligne** : adresse du site, clé de la borne (Enregistrer /
   Oublier la clé), **Tester** (ping).
2. Ligne d'un événement enregistré → **Envoyer en ligne** : préparation du ZIP (dans
   `BoothSoft/uploads/`) et vérification (sha256), avec barre de progression.
3. Code à 6 chiffres → le site répond avec la réservation visée : « Envoyer les photos de
   Léa (12/12/2026) ? » → **Envoyer**.
4. Envoi par morceaux (taille donnée par le site, 32 Mo), chaque morceau retenté jusqu'à 6 fois
   après une coupure réseau ; **Arrêter l'envoi** possible.
5. Le site publie en tâche de fond ; la borne interroge l'état toutes les 3 s.
6. **Galerie en ligne** : nombre de photos et vidéos, QR code et lien. Le ZIP temporaire est effacé.

Un code refusé ramène au pavé avec le message du site ; toute autre erreur affiche le message
et propose **Recommencer** (sans refaire le ZIP).

## Protocole HTTP

Toutes les requêtes : `Authorization: Bearer bsk_…`. Réponses JSON ; en erreur
`{ "error": "message à afficher" }` (le message du site est affiché tel quel).

| Requête | Corps / en-têtes | Réponse |
|---|---|---|
| `GET /api/booth/ping` | — | `{ booth, twoFactor }` |
| `POST /api/booth/uploads` | `{ totp, fileName, size, sha256 }` | `{ uploadId, uploadToken, chunkSize, chunks, reservation: { reference, client, startDate, endDate } }` |
| `PUT /api/booth/uploads/{id}/chunks/{i}` | `X-Upload-Token`, octets bruts | `{ received, chunks }` |
| `POST /api/booth/uploads/{id}/complete` | `X-Upload-Token` | `202 { status: "processing" }` |
| `GET /api/booth/uploads/{id}` | `X-Upload-Token` | `{ status: open|processing|done|failed, url?, photos?, videos?, error? }` |

Codes : 401 clé ou code refusé, 404 introuvable, 409 déjà terminé / morceaux manquants,
410 expiré, 413 morceau trop gros, 422 ZIP refusé, 429 trop de requêtes. Les erreurs réseau,
429 et 5xx sont retentées ; les autres arrêtent l'envoi.

## Code

- `src/gallery_upload.rs` : réglages (adresse, clé), `ping`, `prepare` (ZIP + sha256),
  `start`, `send` (morceaux, publication, attente), `qr_modules`. Client HTTP `ureq`
  (bloquant, rustls avec certificats racine embarqués : rien à installer sur le PC de la borne).
- `src/kiosk/admin/upload.rs` : carte de réglages, bouton par événement, fenêtre d'envoi
  (pavé, confirmation, progression, QR code dessiné en rectangles, sans texture ni animation).
- Test de bout en bout contre une copie locale du site : `live_site` (ignoré par défaut, voir le
  commentaire dans `src/gallery_upload.rs`).

## Vérifié / non vérifié

Vérifié sous Linux (aarch64, Xvfb, rendu logiciel, build de développement) contre une copie
locale du site : envoi d'un ZIP en 1 et en 3 morceaux, code 2FA réel, galerie publiée,
parcours complet à l'écran avec captures. Mesure : 2,3 % CPU pendant l'envoi et l'écran final
(étape de 5 s), 135 Mo de RAM.

Non vérifié : Windows, macOS, vraie connexion Wi-Fi d'une salle, très gros ZIP (plusieurs Go)
et reprise après une vraie coupure.
