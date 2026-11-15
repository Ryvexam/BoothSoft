# BoothSoft — guide pour l'agent qui reprend le développement

Tu reprends une application existante et fonctionnelle. Lis ce fichier en entier avant
de toucher au code, puis les documents listés au § 7 selon ta tâche.

## 1. Le produit en trois phrases

BoothSoft est un logiciel **open source** (CPAL-1.0 : crédit « BoothSoft, créé par Dôm'Booth » obligatoire à l'écran) de borne photo de location :
les invités d'un mariage ou d'un anniversaire touchent l'écran, prennent la pose,
et récupèrent un tirage 10×15. La borne fonctionne **hors ligne**, **sans personne de
l'exploitant sur place ni à distance**, sur un petit PC. Elle doit être aussi simple que
LumaBooth / dslrBooth, mais beaucoup plus légère.

### Trois notions à ne jamais confondre

| Notion | Définition |
| --- | --- |
| **Thème** | L'habillage de la borne aux couleurs de l'entreprise qui l'exploite : couleurs, logo, accueil, textes. Il ne change pas d'un événement à l'autre. Il n'existe pas de thème « mariage ». |
| **Événement** | Une prestation chez un client : nom, date, horaires, tirages prévus, et **son cadre**. |
| **Cadre** | L'image posée sur la photo imprimée. Il appartient à l'événement : chaque événement a le sien. La photo est placée dans la **zone transparente du cadre** (détectée automatiquement, `compositor::detect_window`) ; un cadre transparent jusqu'aux bords donne une photo pleine page. Un cadre paysage (1800×1200) et un cadre portrait (1200×1800). |

Le code actuel range encore le cadre par défaut dans le thème (ou un PNG importé en
maintenance) : les cadres par événement arrivent avec `docs/specs/EVENT_FRAMES.md`.

### Projet open source : ce qui est public, ce qui ne l'est pas

| Public (dans le dépôt, livré à tous) | Privé (jamais dans le dépôt) |
| --- | --- |
| Tout le code, les documents, les scripts | Le thème de la marque de chaque exploitant : logo, couleurs, illustrations |
| **Un seul thème : « Party »** (`themes/default`), générique et festif | Les cadres, photos, réglages et plannings des clients |
| Les cadres génériques de la bibliothèque | Le kit Canon EDSDK (licence Canon, non redistribuable) |
| Les polices sous licence libre (OFL) | Tout nom de client, toute photo d'invité |

Le kit Canon et le thème de marque vivent dans un **dépôt privé qui dépend du dépôt
public**, jamais l'inverse : pas d'option de compilation `canon`, pas de dépendance
facultative, pas de nom de fabricant dans ce dépôt. Organisation : `docs/EXTENSIONS.md`.

Refaire les captures et la démonstration du README : thème Party, caméra
`BOOTHSOFT_SYNTHETIC_CAMERA=guests`, `scripts/perf/monitor.py` pour les clics et
`ffmpeg -f x11grab` pour la vidéo ; fichiers dans `docs/media/`.

- Le thème d'une marque s'installe comme n'importe quel thème d'utilisateur : dossier
  `BoothSoft/themes/<id>/` ou import d'un ZIP. Le code ne doit **jamais** supposer un
  thème de marque présent, ni contenir un nom de marque ou d'exploitant dans un texte,
  un identifiant ou une valeur par défaut.
- Une installation neuve démarre sur le thème « Party » et un cadre générique.
- Les exemples des documents et des tests utilisent des noms fictifs (« Léa & Hugo »)
  et le thème « Party ».

## 2. Matériel de référence

| Composant | Référence |
| --- | --- |
| Appareil photo | Canon EOS 2000D, relié en USB (pas encore pris en charge : fiche A1 de la feuille de route) |
| Imprimante | DNP QW410 (sublimation thermique, 300 dpi), tirage 10×15 uniquement, une photo par tirage |
| Écran | 15,6 pouces tactile, 16:9 (1920×1080 supposé), **aucun clavier ni souris** |
| PC | Mini-PC Intel Core i5, 8 Go de RAM, SSD 256 Go, Windows |

Le développement se fait sur macOS (Mac du propriétaire) et se vérifie sur Linux
(Xvfb + rendu logiciel). La cible finale est Windows.

## 3. Règles non négociables

0. **Tout texte visible passe par `tr!(fr: "…", en: "…")`** (`src/i18n.rs`). Jamais de
   texte d'interface en dur dans une seule langue. Les lignes du journal restent en anglais.

1. **Tout en Rust.** Pas de moteur web, pas de script embarqué, pas de service externe.
   Les seuls scripts hors Rust sont des outils de développement (`scripts/`).
2. **0 % de CPU au repos.** Un écran statique ne demande jamais de redessin. Aucune
   animation en boucle. Les animations sont courtes (≤ 250 ms), déclenchées par un
   toucher ou un changement d'écran, et s'arrêtent seules (`ui::appear`, `ui::glide`,
   `ui::press` dans `src/kiosk/ui.rs`).
3. **Rien de lent sur le fil d'interface** : pas de décodage ou d'encodage d'image, pas
   d'accès disque lourd, pas de sous-processus, pas de hachage. Fil de travail + canal +
   réveil de l'interface par `request_repaint_after(1 ms)` (jamais `request_repaint()`
   depuis un fil de travail : egui dessine alors deux images).
4. **Budgets** (détail dans `docs/ROADMAP.md` § 1) : RAM ≤ 150 Mo au repos, live 30 img/s
   ≤ 15 % d'un cœur avec puce graphique, ≤ 50 % sans.
5. **Pensé pour le doigt** : cibles d'au moins 12 mm (≈ 67 px en 1080p sur 15,6"),
   aucune action qui exige un clavier ou une souris côté invité ; côté maintenance,
   tout doit rester faisable au doigt.
6. **Zéro intervention** : aucun écran sans issue, toute erreur se retente seule, on peut
   toujours prendre une photo. Ne jamais écrire « appelez l'équipe ».
7. **Mesurer** : chaque changement visible se termine par une mesure avant / après
   avec `scripts/perf/monitor.py`, ajoutée à `docs/GUEST_UI.md`.
8. **Textes invités en français** aujourd'hui, apostrophe typographique (’), ton
   chaleureux et court ; la maintenance aussi. Un système de langues est prévu (anglais
   par défaut, français, espagnol, chinois : `docs/specs/LANGUAGES.md`). D'ici là, garde
   chaque texte d'interface entier et isolé (pas de phrase assemblée par morceaux) pour
   faciliter son extraction.
9. **Sécurité des fichiers** : un thème est un dossier validé, jamais un chemin libre
   (`src/theme_config.rs`). Écritures de réglages atomiques (`src/durable_file.rs`).
10. **Rien n'est perdu ni enregistré en silence** dans la maintenance : toute sortie
    avec des modifications passe par la fenêtre « Quitter sans enregistrer ? » ; les
    3 dernières versions des réglages, thèmes et cadres sont conservées.
11. **Décisions déjà prises, à ne pas rouvrir** : thème ≠ cadre ≠ événement (voir § 1) ;
    la photo remplit tout le tirage sous le cadre ; pas de partage par QR code / e-mail /
    SMS, pas de bandes 2×6 ni de montage à plusieurs photos, pas de fond vert, pas de
    vidéo d'attraction, pas de cloud. Voir `docs/ROADMAP.md` § 7.

## 4. Commandes

```sh
cargo build --release                       # compilation
cargo run --release                         # lancement (plein écran, thème enregistré)
cargo run --release -- --theme default      # forcer le thème livré « Party »
cargo run --release -- --demo --windowed    # sans caméra, en fenêtre
cargo test --release                        # tests (63 + 7 au 11/11/2026)
cargo clippy --release --all-targets -- -D warnings -D dead_code   # doit passer sans rien
scripts/build-booth.sh                      # binaire optimisé pour le PC de la borne
target/release/boothsoft-guard --demo --windowed   # l'app sous son gardien (relance auto)
```

Variables d'environnement utiles :

| Variable | Effet |
| --- | --- |
| `BOOTHSOFT_SYNTHETIC_CAMERA=1`, `static` ou `guests` | Caméra simulée (flux MJPEG 1280×720) : forme animée (référence des mesures), scène fixe, ou deux invités dessinés (captures et démonstrations) |
| `BOOTHSOFT_FPS_LOG=1` | Affiche le nombre de redessins par seconde et leur cause |
| `BOOTHSOFT_LIVE_CPU=1` | Force le live par textures egui (sans le chemin OpenGL direct) |
| `LP_NUM_THREADS` | Fils du rendu logiciel Mesa (fixé à 1 au démarrage si absent) |

Vérification visuelle et mesure sans écran (Linux) :

```sh
Xvfb :99 -screen 0 1280x800x24 &
python3 scripts/perf/monitor.py target/release/boothsoft scripts/perf/steps-flow.json /tmp/sortie
# BS_DATA=/chemin  → dossier de données à utiliser (réglages préparés pour un scénario)
# NODEMO=1         → ne pas ajouter --demo
```

Le fichier d'étapes est une liste JSON `{"name", "click":[x,y], "key", "dwell", "shot"}`.
Les touches clavier ne passent pas sans gestionnaire de fenêtres : n'utilise que des clics.
Accès maintenance dans un scénario : 5 clics rapides en (50, 50) sur le live.

## 5. Carte du code (≈ 7 900 lignes)

| Fichier | Rôle |
| --- | --- |
| `src/main.rs` | Arguments, choix du thème, fenêtre eframe |
| `src/kiosk.rs` | État de la borne, machine à écrans, tous les écrans invités |
| `src/kiosk/ui.rs` | Boîte à dessin : polices, palette, boutons, icônes, animations |
| `src/kiosk/live_gl.rs` | Live vidéo en OpenGL direct (texture + shader : filtre, miroir, recadrage) |
| `src/kiosk/burst.rs` | Mode rafale : prise de vue en série et écran de choix |
| `src/kiosk/admin.rs` | Maintenance : code PIN, pages de réglages, thèmes, quota |
| `src/camera.rs` | Fil caméra, sources (`FrameSource`), détection de mouvement et de présence |
| `src/mjpeg.rs` | Décodage JPEG rapide (libjpeg-turbo), tolérant aux images corrompues |
| `src/photo_worker.rs` | Fil photo : filtre, enregistrement, composition avec le cadre |
| `src/compositor.rs` | Recadrage, redimensionnement (SIMD), pose du cadre PNG |
| `src/effects.rs` | Les 6 filtres (matrices couleur) |
| `src/printer.rs` | Fil d'impression, imprimante par défaut, envoi au système |
| `src/print_jobs.rs`, `src/print_quota.rs` | Journal des tirages, quota par événement |
| `src/storage.rs` | Dossier de données, séances, photos par événement |
| `src/model.rs` | Réglages (`Settings`), planning (`EventSlot`), écrans (`Screen`) |
| `src/theme.rs`, `src/theme_config.rs`, `src/theme_import.rs` | Thème actif, format et validation, import ZIP |
| `src/autostart.rs` | Lancement au démarrage de l'ordinateur (lance le gardien s'il existe) |
| `src/log.rs` | Journal sur disque (`BoothSoft/logs/`), fil `bs-log` |
| `src/watchdog.rs`, `src/bin/boothsoft-guard.rs` | Gardien : relance après plantage ou gel, mode sans échec |
| `src/devices.rs` | Recherche des caméras et imprimantes (fil dédié) |
| `src/bin/` | `boothsoft-bench`, `boothsoft-doctor`, `boothsoft-theme-check` |
| `themes/` | Seul thème livré : `default` (« Party »), avec un cadre paysage et un cadre portrait |
| `scripts/theme-art/generate.py` | Génération des illustrations des thèmes livrés (outil de dev) |

Détail des fils, des écrans et des données : `docs/ARCHITECTURE.md`.

## 6. Pièges déjà rencontrés (ne les redécouvre pas)

- `request_repaint()` sans délai = 2 images dessinées. Depuis un fil : `request_repaint_after(1 ms)`.
- Changer la taille d'un texte à chaque image fait grossir l'atlas de polices (+24 Mo
  mesurés). Pour animer un texte, anime sa **position** ou dessine-le dans une texture
  (`ui::BigNumeral`), jamais sa taille.
- `image::imageops::thumbnail(w, h)` déforme. Utilise `compositor::fit_within`.
- `Stroke::new(1.0, …)` déclenche un avertissement de compatibilité : écris `1.0_f32`.
- Dans une ligne egui alignée à droite (`Layout::right_to_left`), les éléments sont
  posés à l'envers : donne une largeur fixe aux groupes (voir `segmented` dans `admin.rs`).
- `ctx.animate_bool` ne joue pas d'animation au premier appel : pour une entrée, utilise `ui::appear`.
- Profil release : `panic = "unwind"` est obligatoire (libjpeg signale une image
  corrompue par déroulement de pile, rattrapé dans `mjpeg::decode`).
- Les fichiers des thèmes livrés sont en lecture seule : un thème livré ne se modifie
  pas, il se duplique dans le dossier utilisateur.
- EDSDK (Canon) impose que tous ses appels viennent du même fil : le fil `bs-camera`.
- Le rendu logiciel coûte au nombre de pixels dessinés : évite les aplats plein écran
  superposés au live et les calques translucides redessinés à chaque image.

## 7. Documents

| Document | Contenu | Quand le lire |
| --- | --- | --- |
| `docs/ROADMAP.md` | Budgets, écart avec dslrBooth, tous les chantiers classés, tableau des pannes | Toujours, pour savoir quoi faire et dans quel ordre |
| `docs/ARCHITECTURE.md` | Fils, écrans, formats de données, chaîne photo et impression | Avant toute modification |
| `docs/GUEST_UI.md` | Parcours invité, règles de rendu, toutes les mesures | Avant de toucher à l'interface ou aux performances |
| `docs/EXTENSIONS.md` | Comment le kit Canon et le thème de marque restent dans un dépôt privé | Chantier A1, et avant de toucher à `camera.rs` |
| `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `NOTICE`, `LICENSE` | Vitrine et règles du projet open source | Avant toute publication |
| `docs/specs/MAINTENANCE_PATTERNS.md` | Vocabulaire, fenêtre « Quitter sans enregistrer ? », historique à 3 versions, brouillons | Chantier M1, et avant tout travail dans la maintenance |
| `docs/specs/EVENT_FRAMES.md` | Un cadre par événement : bibliothèque, choix, fabrication, migration | Chantier F1 |
| `docs/specs/THEME_EDITOR.md` | Éditeur du thème de l'entreprise | Chantier T1 |
| `docs/specs/GUEST_BOOK.md` | Livre d'or vidéo (à débloquer) : test du micro, message vidéo avec le son, AVI MJPEG sans encodage | Chantier G1 |
| `docs/specs/LANGUAGES.md` | Système de langues : anglais par défaut, français, espagnol, chinois | Chantier L1, et avant d'écrire un nouveau texte d'interface |
| `docs/specs/TOUCH_KEYBOARD.md` | Clavier tactile de la maintenance | Chantier K1 (prérequis de la saisie de texte sur la borne) |

Ordre conseillé des chantiers de personnalisation : M1.1 → F1.1 → F1.2 → F1.3 → K1 →
M1.2 → T1.1 → T1.2 → F1.4 → T1.3 → T1.4 → M1.3.

## 8. Façon de travailler attendue

1. Un chantier = une fiche de `docs/ROADMAP.md` ou un lot d'une spécification.
   Ne mélange pas deux chantiers.
2. Avant de coder : relis la fiche, repère les fichiers cités, lance les tests.
3. Pendant : petits pas, compilation et clippy à chaque étape.
4. Après : tests, clippy sans avertissement, scénario Xvfb avec captures d'écran que tu
   regardes vraiment, mesure CPU / RAM, mise à jour de `docs/GUEST_UI.md` (mesures) et
   de la fiche (statut « fait », écarts éventuels).
5. Dis clairement ce qui n'a pas pu être vérifié (Windows, vrai Canon, vraie DNP) :
   ne présente jamais comme testé ce qui ne l'a pas été sur le matériel.
6. Si une décision de produit manque, propose un choix par défaut raisonnable, applique-le
   et signale-le ; ne bloque pas sur une question secondaire.
