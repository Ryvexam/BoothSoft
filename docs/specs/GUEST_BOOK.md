# Spécification — Livre d'or vidéo (chantier G1)

Statut : **développé (lots G1.1 à G1.5, novembre 2026)** ; G1.6 (MP4) reste facultatif. Priorité P2. Fonction **à débloquer** dans la maintenance
(désactivée par défaut). Lis d'abord `AGENTS.md`, `docs/ARCHITECTURE.md` (fils, écrans,
live) et `docs/specs/MAINTENANCE_PATTERNS.md`.

## 1. Le besoin

Sur le live, à côté de « Une photo » et « Rafale », un troisième mode **« Livre d'or »**
permet aux invités d'enregistrer un **message vidéo avec le son**, de 30 secondes au
plus, destiné à la personne qui organise l'événement (les mariés, la personne fêtée…).

Parcours invité :

1. Sur le live, l'invité choisit **Livre d'or**.
2. **Test du micro** : « Dites quelque chose ! » — une jauge montre en direct le volume
   capté. Si c'est bon, il touche **Commencer**.
3. Court décompte (3 s), puis **enregistrement** : l'invité parle face à la caméra.
   Il arrête quand il veut (bouton **Terminer**) ; l'enregistrement s'arrête seul à la
   durée maximale (30 s par défaut).
4. **Revoir** son message (lecture avec le son), puis **Garder** ou **Recommencer**.
5. Remerciement : « Votre message a été enregistré pour Léa & Hugo ! », retour au live.

Les messages sont rangés dans le dossier de l'événement et remis à l'organisateur avec
les photos (export).

### Hors périmètre

- Envoi en ligne, partage par QR code (décision produit : hors ligne).
- Impression d'une vignette du message.
- Montage automatique des messages en un seul film (piste future, § 13).
- Enregistrement avec l'appareil reflex en mode vidéo (voir § 6.4 pour ce qui est prévu).

## 2. Décisions de conception

| # | Décision | Raison |
| --- | --- | --- |
| D1 | Fonction désactivée par défaut, activée dans la maintenance (« Livre d'or vidéo »), et activable événement par événement quand la fiche C3 existera. | Le loueur la vend en option. |
| D2 | **Aucun encodage vidéo pendant l'enregistrement** : les images JPEG que la caméra envoie déjà (MJPEG de la webcam, visée JPEG du Canon) sont écrites telles quelles dans le fichier. | Coût CPU quasi nul sur l'i5-4210M ; aucune bibliothèque de codec ; aucun brevet. |
| D3 | Conteneur **AVI** (RIFF), vidéo **MJPEG**, son **PCM 16 bits mono 48 kHz**, écrit par notre propre code (≈ 300 lignes). Durée ≤ 30 s → fichier < 1 Go, pas besoin d'OpenDML. | Format simple, lu par VLC, le lecteur Films et TV de Windows et la plupart des logiciels de montage. Écrit en flux : aucune vidéo gardée en mémoire. |
| D4 | Conversion en **MP4** (H.264 + AAC) : **pas dans cette version**. Lot facultatif plus tard (§ 12, lot G1.6), faite la nuit ou à l'export, jamais pendant l'événement. | Le format AVI suffit pour remettre les messages ; le MP4 demande un encodeur (licences, CPU). |
| D5 | Capture du son avec la bibliothèque **`cpal`** (Rust, WASAPI sous Windows, CoreAudio sous macOS, ALSA sous Linux). Seule nouvelle dépendance de ce chantier. | Multiplateforme, libre (Apache-2.0), sans service externe. |
| D6 | Le micro n'est **ouvert que** sur les écrans du livre d'or (test, enregistrement) et fermé dès qu'on en sort. Jamais d'écoute en arrière-plan. | Vie privée, CPU, et le micro d'une webcam peut être partagé. |
| D7 | Le message est enregistré **sans effet miroir** (l'image montrée à l'invité, elle, reste en miroir, comme pour les photos). | Un texte ou un geste vers la droite reste juste à la relecture. |
| D8 | Aucun filtre de couleur sur la vidéo (les filtres restent pour les photos). | Écrire les JPEG tels quels (D2). |
| D9 | Pas de quota, pas d'impression : le livre d'or ne consomme aucun tirage. | Simplicité. |
| D10 | Mention à l'écran que le message est enregistré et remis à l'organisateur, avant le début de l'enregistrement. | Les invités savent qu'ils sont filmés et pour qui. |

## 3. Matériel

| Élément | Exigence |
| --- | --- |
| Micro | Micro de la webcam, ou micro USB dédié (recommandé : meilleure prise de son dans le bruit d'une soirée). Choisi dans la maintenance. |
| Caméra | La même que pour les photos. Webcam : flux MJPEG 1280×720 à 30 img/s (déjà utilisé pour le live). Canon 2000D : visée JPEG 1056×704 (voir § 6.4). |
| Disque | ≈ 2 à 4 Mo par seconde de message (MJPEG 720p), soit ≈ 60 à 120 Mo par message de 30 s. 100 messages ≈ 6 à 12 Go. Le contrôle d'espace disque (fiche Z5) s'applique. |
| Son de relecture | Haut-parleurs du PC ou de l'écran. Volume réglable dans la maintenance. |

## 4. Réglages (maintenance)

Nouvelle carte **Livre d'or vidéo** dans la page Événements (après « Séance photo ») :

| Réglage | Composant | Défaut | Détail |
| --- | --- | --- | --- |
| Livre d'or vidéo | `toggle` | désactivé | Affiche le mode sur le live. |
| Durée maximale | `stepper` 10 → 30 s, pas de 5 s | 30 s | « 0 à 30 s au choix » : l'invité arrête quand il veut, la borne arrête à cette durée. |
| Durée minimale | — | 2 s | Fixe. Un message plus court est jeté (toucher accidentel). |
| Destinataire | Champ de texte | vide | Affiché aux invités : « Un message pour {destinataire} ». Vide → nom de l'événement en cours. |
| Micro | Liste déroulante | micro par défaut du système | Liste des entrées audio (`cpal`), avec « Par défaut du système ». Bouton « Tester » : jauge de volume dans la carte. |
| Sensibilité | `segmented` : Faible · Normale · Forte | Normale | Gain logiciel ×1 / ×2 / ×4 appliqué au son enregistré (avec écrêtage doux). |
| Volume de relecture | `stepper` 0–100 % | 80 % | |

Modèle (`src/model.rs`, tous avec `#[serde(default)]`) :

```rust
pub guestbook_enabled: bool,          // false
pub guestbook_max_seconds: u8,        // 30, normalisé 10..=30
pub guestbook_recipient: String,      // "", 80 caractères au plus
pub guestbook_mic: String,            // "" = défaut du système
pub guestbook_gain: u8,               // 1 | 2 | 4
pub guestbook_playback_volume: u8,    // 0..=100
```

Page Données : la carte « Photos » devient « Photos et messages » et ajoute le nombre de
messages et leur durée totale pour l'événement en cours.

## 5. Parcours et écrans

### 5.1 Sélecteur de mode sur le live

Le sélecteur actuel (`kiosk.rs::paint_mode_switch`) passe à trois segments quand le
livre d'or est activé :

```
   ┌──────────────────────────────────────────────────┐
   │ ( Une photo )   Rafale ×5    🎤 Livre d'or        │
   └──────────────────────────────────────────────────┘
```

- **« Une photo » est toujours présélectionné** (au démarrage et après chaque séance).
- Les segments affichés dépendent des réglages : rafale proposée ou non, livre d'or
  activé ou non. Avec un seul mode disponible, pas de sélecteur.
- Remplacer le booléen `guest_burst` par `enum GuestMode { Photo, Burst, GuestBook }`.
- Choisir « Livre d'or » change le déclencheur : icône micro et libellé « Laisser un
  message ». Les vignettes de filtres sont masquées (pas de filtre en vidéo).
- Toucher le déclencheur en mode Livre d'or ouvre l'écran **Test du micro** (pas de décompte).

### 5.2 Nouveaux écrans

`model::Screen` gagne : `MicCheck`, `Recording`, `MessageReview`. Les écrans existants
`Countdown` (réutilisé, 3 s) et `Thanks` (texte adapté) servent aussi.

```
 Ready ──(mode Livre d'or, déclencheur)──▶ MicCheck ──Commencer──▶ Countdown (3 s)
                                              │                          │
                                         Retour / 45 s                   ▼
                                              ▼                      Recording ──Terminer / durée max──▶ MessageReview
                                            Ready                                                         │      │
                                                                                         Recommencer ◀────┘      └── Garder ──▶ Thanks ──▶ Ready
```

### 5.3 Test du micro (`Screen::MicCheck`)

```
┌──────────────────────────────────────────────────────────────────────────┐
│ ←                         Un message pour Léa & Hugo                      │
│                                                                          │
│        ┌───────────────────────────────┐                                 │
│        │                               │    Dites quelque chose !        │
│        │   live (miroir), 16:9,         │                                 │
│        │   60 % de la largeur           │    🎤  ▮▮▮▮▮▮▮▮▯▯▯▯▯▯▯▯          │
│        │                               │        « On vous entend bien ! » │
│        └───────────────────────────────┘                                 │
│                                                                          │
│   Votre message vidéo (30 s au plus) sera remis à Léa & Hugo.            │
│                                                                          │
│                                          [  Commencer l'enregistrement  ] │
└──────────────────────────────────────────────────────────────────────────┘
```

- **Jauge** : 16 segments, niveau = valeur crête des 100 dernières millisecondes
  convertie en décibels (−60 dB → 0 segment, 0 dB → 16), avec retombée douce
  (−24 dB/s). Couleur d'accent ; les 3 derniers segments en orange pour la saturation.
- **Message sous la jauge**, selon le niveau des 2 dernières secondes :
  - rien au-dessus de −50 dB : « Nous ne vous entendons pas encore : parlez près de l'écran. »
  - pic au-dessus de −35 dB : « On vous entend bien ! »
  - saturation (> −1 dB) répétée : « Un peu moins fort, c'est parfait. »
- **Commencer** est toujours actif (on ne bloque pas un invité timide), mais il est mis
  en avant (contour animé à l'apparition, une seule fois) dès que « On vous entend bien ! »
  s'affiche.
- **Micro absent ou en erreur** : la jauge est remplacée par « Le micro ne répond pas »
  et un bouton « Enregistrer sans le son » (le fichier n'aura pas de piste audio).
  L'erreur est journalisée ; la borne retente l'ouverture du micro toutes les 2 s.
- **Rafraîchissement** : cet écran est le seul à se redessiner en continu en dehors du
  live : jauge à **15 images/s** au plus (`request_repaint_after(66 ms)`), et le live
  comme d'habitude. Rien quand l'écran n'est pas affiché.
- Retour (←) ou 45 s sans toucher : micro fermé, retour au live.

### 5.4 Enregistrement (`Screen::Recording`)

- Live plein écran (miroir pour l'invité), comme pendant une photo.
- En haut : pastille rouge « ● REC » qui pulse — **seule animation en boucle autorisée
  du projet**, justifiée parce que l'écran se redessine déjà à chaque image du live :
  elle ne coûte aucun redessin supplémentaire.
- Anneau de progression autour du bouton **Terminer** (même dessin que le décompte),
  temps restant au centre (« 0:24 »).
- Jauge de volume compacte (8 segments) en bas à gauche.
- **Terminer** : actif après 2 s. Avant, toucher n'a pas d'effet.
- Durée maximale atteinte : arrêt automatique avec un court flash « C'est dans la boîte ! ».
- Pendant l'enregistrement, la borne ne s'endort pas et la présence n'est pas testée.
- Si l'écriture du fichier échoue (disque plein…) : arrêt propre, message « Le message
  n'a pas pu être enregistré », journal, retour au live après 5 s.

### 5.5 Relecture (`Screen::MessageReview`)

- La vidéo se lit automatiquement une fois, **avec le son**, au centre, sur le fond flou
  habituel (comme l'aperçu photo).
- Boutons : **Recommencer** (efface le fichier, retour à l'écran de test), **Garder**
  (principal). Un bouton « Revoir » apparaît à la fin de la lecture.
- Retour automatique au bout de `photo_timeout_seconds` sans toucher : le message est
  **gardé** (un message enregistré n'est jamais perdu par inaction).
- Lecture : voir § 7.

### 5.6 Remerciement

`render_thanks` avec la première image du message à la place du tirage, titre
« Merci pour votre message ! », sous-titre « Il sera remis à Léa & Hugo. ».
Pas de compteur de tirages.

## 6. Capture

### 6.1 Vidéo

- Le fil caméra (`bs-camera`) reçoit une nouvelle commande
  `CameraCommand::Record(Option<RecordSink>)`. Tant qu'un `RecordSink` est présent, il
  **copie les octets JPEG bruts** de chaque image reçue (avant décodage) dans un canal
  borné vers le fil d'écriture, avec l'instant de capture. Le décodage pour le live
  continue comme aujourd'hui.
- Pas de passage par `MotionGate` pour l'enregistrement : toutes les images sont gardées.
- Si la source n'est pas MJPEG (webcam en YUYV, caméra simulée sans JPEG…), le fil
  caméra encode l'image décodée en JPEG qualité 80 avec `mozjpeg` (déjà présent) —
  plus coûteux, à mesurer ; la webcam de la borne doit être réglée en MJPEG.
- Cadence : celle de la caméra (30 img/s visés). Les horodatages servent à calculer la
  cadence réelle écrite dans l'en-tête AVI à la fermeture.

### 6.2 Son

- Module `src/audio.rs` : `pub struct MicStream` ouvert par `MicStream::open(device_name)`,
  fermé à la destruction (`Drop`). Format demandé : 48 kHz mono 16 bits ; si le
  périphérique ne le propose pas, prendre son format par défaut et convertir
  (mélange des canaux en mono, rééchantillonnage linéaire vers 48 kHz).
- Le rappel audio de `cpal` (fil temps réel, ne rien y allouer) :
  1. calcule la crête du bloc et la publie dans un `AtomicU32` (niveau pour la jauge) ;
  2. si l'enregistrement est actif, pousse les échantillons dans un tampon circulaire
     sans verrou (capacité 2 s) lu par le fil d'écriture.
- Gain logiciel (réglage Sensibilité) appliqué par le fil d'écriture, avec écrêtage doux.
- `pub fn list_inputs() -> Vec<String>` pour la maintenance, appelé depuis le fil de
  recherche d'appareils (`devices.rs`).

### 6.3 Écriture : `src/avi.rs` et fil `bs-record`

- `AviWriter::create(path, width, height, has_audio)` ; `write_video(jpeg: &[u8])` ;
  `write_audio(pcm: &[i16])` ; `finish(real_fps)` qui écrit l'index `idx1` et corrige
  les en-têtes (nombre d'images, cadence, durée).
- Structure : `RIFF('AVI ' LIST('hdrl' avih LIST('strl' strh(vids, 'MJPG') strf(BITMAPINFOHEADER))
  LIST('strl' strh(auds) strf(WAVEFORMATEX PCM 48000 Hz 1 canal 16 bits))) LIST('movi' 00dc… 01wb…) idx1)`.
- Entrelacement : un bloc audio d'environ 1/30 s (1 600 échantillons) après chaque image.
- Fichier écrit sous un nom provisoire `.part`, renommé à la fin (`finish`). Un fichier
  `.part` trouvé au démarrage est réparé si possible (index reconstruit), sinon supprimé
  et journalisé.
- Le fil `bs-record` vit le temps d'un enregistrement. Canal vidéo borné à 30 images
  (1 s) ; si plein, l'image est sautée et comptée (journal).
- Dossier : `sessions/<dossier-événement>/livre-d-or/message-AAAAMMJJ-HHMMSS.avi` +
  `message-…-apercu.jpg` (première image, pour la maintenance et l'écran de remerciement)
  + `message-….json` (durée, date, nombre d'images sautées, micro utilisé).
- Après « Recommencer », les trois fichiers sont supprimés. Après « Garder », ils restent.

### 6.4 Canon EOS 2000D

La visée en direct du Canon fournit des JPEG 1056×704 (voir fiche A1 de la feuille de
route) : le même principe s'applique sans changement (copie des JPEG de visée). Le son
vient d'un micro séparé (le 2000D ne transmet pas le son par USB). Cadence réelle de la
visée à mesurer ; si elle est inférieure à 20 img/s, l'indiquer dans la maintenance.
La source Canon vit dans le dépôt privé (`docs/EXTENSIONS.md`) : le contrat
`FrameSource` doit donc exposer les octets JPEG bruts (`fn last_jpeg(&self) -> Option<&[u8]>`).

## 7. Relecture

- Fil `bs-playback` le temps de la relecture : lit le fichier AVI (notre format
  uniquement, via l'index), décode chaque JPEG avec `mjpeg::decode` (réglage « live »,
  rapide) au rythme des horodatages, et l'envoie au live GL existant (`LiveView::submit`)
  — même chemin d'affichage que la caméra, donc aucun nouveau code de rendu.
- Son : flux de sortie `cpal` sur le périphérique par défaut, alimenté par les blocs PCM
  du fichier ; l'horloge audio cadence la vidéo (une image est montrée quand l'audio a
  atteint son horodatage).
- Volume : réglage de la maintenance.
- Fin de lecture : dernière image figée, bouton « Revoir ».
- Pendant la relecture, la caméra continue de tourner mais ses images ne sont pas
  affichées (évite deux flux concurrents dans la texture du live).

## 8. Maintenance : écouter et exporter les messages

Nouvelle page **Livre d'or** dans le menu (visible si la fonction est activée ou si
des messages existent) :

- Liste des messages de l'événement sélectionné (sélecteur d'événement en haut) :
  vignette, heure, durée, bouton ▶ (lecture dans une fenêtre, même lecteur qu'au § 7),
  bouton Supprimer (confirmation, `MAINTENANCE_PATTERNS.md` § 5).
- « Exporter les messages » : copie les fichiers `.avi` dans le dossier d'export choisi
  (clé USB), avec un fichier `LISEZ-MOI.txt` indiquant d'ouvrir les vidéos avec VLC.
  Copie dans un fil de travail, barre de progression.

## 9. Performances

| Mesure | Budget (i5-4210M avec puce graphique) |
| --- | --- |
| CPU écran de test du micro | ≤ live + 3 % |
| CPU pendant l'enregistrement (webcam MJPEG) | ≤ live + 5 % (copie d'octets et écriture disque seulement) |
| CPU pendant la relecture | ≤ live (un décodage JPEG par image, comme le live) |
| RAM supplémentaire | ≤ 15 Mo (canal de 30 images JPEG ≈ 6 Mo, tampon audio 2 s, lecteur) |
| Écriture disque | ≈ 2 à 4 Mo/s pendant 30 s |
| Repos (livre d'or activé, autre écran) | 0 %, micro fermé |

## 10. Vie privée

- Bandeau sur l'écran de test : « Votre message vidéo sera enregistré et remis à
  {destinataire}. » (D10).
- Les messages ne quittent la borne que par l'export manuel de l'exploitant.
- Supprimer un événement ou ses photos (fiches futures) supprime aussi ses messages.
- Aucun message dans le dépôt ni dans les tickets (`CONTRIBUTING.md`).

## 11. Textes (français ; à faire passer par le système de langues, `LANGUAGES.md`)

| Endroit | Texte |
| --- | --- |
| Sélecteur | « Livre d'or » |
| Déclencheur | « Laisser un message » |
| Test, titre | « Un message pour {destinataire} » |
| Test, consigne | « Dites quelque chose ! » |
| Test, retours | « On vous entend bien ! » · « Nous ne vous entendons pas encore : parlez près de l'écran. » · « Un peu moins fort, c'est parfait. » · « Le micro ne répond pas. » |
| Test, bouton | « Commencer l'enregistrement » · « Enregistrer sans le son » |
| Enregistrement | « ● REC » · « Terminer » · « C'est dans la boîte ! » |
| Relecture | « Votre message » · « Recommencer » · « Garder » · « Revoir » |
| Remerciement | « Merci pour votre message ! » · « Il sera remis à {destinataire}. » |
| Erreur | « Le message n'a pas pu être enregistré. » |

## 12. Lots

| Lot | Contenu | Effort |
| --- | --- | --- |
| G1.1 — Écriture AVI | `src/avi.rs` + tests : fichier lu par `ffprobe` (outil de test seulement) avec la bonne durée, la bonne cadence et les deux pistes ; réparation d'un `.part`. | M |
| G1.2 — Son | `cpal`, `src/audio.rs`, liste des micros, jauge, gain, test du micro dans la maintenance. Mesure CPU / RAM. | M |
| G1.3 — Parcours invité | `GuestMode`, sélecteur à trois modes, écrans `MicCheck`, `Recording` (copie des JPEG, fil `bs-record`), remerciement. | L |
| G1.4 — Relecture | Lecteur AVI + son, écran `MessageReview`. | M |
| G1.5 — Maintenance | Carte de réglages, page Livre d'or (liste, lecture, suppression, export). | M |
| G1.6 — MP4 (facultatif) | Conversion H.264 + AAC hors événement. Choix de l'encodeur à étudier (licences). | L |

Ordre : G1.1 → G1.2 → G1.3 → G1.4 → G1.5. Chaque lot : tests, clippy, scénario Xvfb
(caméra simulée + micro simulé : `BOOTHSOFT_SYNTHETIC_MIC=1` qui génère un son de test,
à ajouter au lot G1.2), mesures, captures regardées.

## 13. Pistes futures

- Montage automatique de tous les messages d'un événement en un seul film, avec un
  carton titre (nuit suivant l'événement).
- Livre d'or activable par événement (fiche C3).
- Questions affichées à l'invité (« Un souvenir avec les mariés ? ») tirées au hasard.

## 14. Critères d'acceptation

- [ ] Désactivé par défaut ; une fois activé, « Livre d'or » apparaît sur le live à
      côté de « Une photo » (présélectionné) et « Rafale ».
- [ ] La jauge réagit à la voix en moins de 100 ms ; « Commencer » lance l'enregistrement.
- [ ] Message de 2 à 30 s (maximum réglable), arrêt par l'invité ou automatique.
- [ ] Le fichier `.avi` se lit avec le son dans VLC et dans le lecteur de Windows 11.
- [ ] Relecture avec le son sur la borne ; Recommencer efface ; Garder conserve.
- [ ] Micro fermé hors des écrans du livre d'or (vérifié par l'indicateur micro du système).
- [ ] Coupure de courant pendant un enregistrement : au redémarrage, le message est
      réparé ou supprimé, jamais un fichier illisible laissé dans le dossier.
- [ ] Budgets du § 9 tenus ; 0 % au repos ; tests et clippy passent.

## 15. État de la réalisation (G1.1 → G1.5)

| Élément | Où | Notes |
| --- | --- | --- |
| Écriture / lecture / réparation AVI | `src/avi.rs` | Tests : relecture, `ffprobe` (MJPEG + `pcm_s16le` 48 kHz, durée exacte), fichier sans son, réparation d'un `.part` coupé net. |
| Micro, haut-parleur, micro simulé | `src/audio.rs` | `cpal` 0.15. `BOOTHSOFT_SYNTHETIC_MIC=1` : voix synthétique. Gain appliqué dans le rappel audio (la jauge montre le niveau enregistré). |
| Fils `bs-record`, `bs-playback`, liste, export | `src/guestbook.rs` | La relecture est cadencée par la position du son ; l'horloge murale prend le relais si la sortie audio cale. |
| Copie des JPEG du live | `src/camera.rs` (`CameraCommand::Record`) | Webcam MJPEG et caméra simulée : octets d'origine ; autre format : réencodage qualité 80. |
| Écrans invité | `src/kiosk/guestbook.rs` | Le décompte de 3 s fait partie de l'écran `Recording` (le `Countdown` des photos n'est pas réutilisé). |
| Maintenance | `src/kiosk/admin.rs` | Carte « Livre d'or vidéo » (Événements), page « Livre d'or » (lecture, suppression avec confirmation, export avec progression), compteur dans Données. |

Mesures (Xvfb 1280×800, rendu logiciel llvmpipe, donc sans GPU : chiffres pessimistes) :
live 38 % ; test du micro 79 % (jauge à 15 img/s **et** live) ; enregistrement 40 % (≈ live) ;
relecture 39 % ; RAM ≈ +10 Mo pendant le parcours. Un message de 3,8 s en 1280×720 :
7,8 Mo (≈ 2 Mo/s), son et vidéo de durée identique (3,80 s).
À mesurer sur la borne (HD 4600) : le test du micro y coûtera bien moins (rendu GPU).
