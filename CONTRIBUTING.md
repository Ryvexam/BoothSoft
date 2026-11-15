# Contribuer à BoothSoft

Merci de votre intérêt. BoothSoft équipe de vraies bornes pendant de vrais mariages :
la fiabilité et la sobriété passent avant les fonctionnalités.

## Avant de commencer

1. Lisez [`AGENTS.md`](AGENTS.md) (règles du projet, carte du code, pièges connus) et
   [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).
2. Regardez la [feuille de route](docs/ROADMAP.md) : beaucoup de chantiers y sont déjà
   décrits, parfois avec une spécification complète dans [`docs/specs/`](docs/specs).
3. Pour une idée nouvelle ou un changement important, ouvrez d'abord un ticket pour en
   discuter.

## Les règles du projet

| Règle | En pratique |
| --- | --- |
| Tout en Rust | Pas de moteur web, pas de script embarqué, pas de service externe. |
| 0 % de processeur au repos | Un écran immobile ne se redessine pas. Aucune animation en boucle ; les animations durent 250 ms au plus et sont déclenchées par un toucher. |
| Rien de lent sur le fil d'interface | Image, disque, sous-processus, hachage : dans un fil de travail, avec un canal et un réveil de l'interface. |
| Pensé pour le doigt | Cibles d'au moins 44 px, pas de survol, pas de clic droit, pas de clavier côté invités. |
| Zéro intervention | Aucun écran sans issue ; toute erreur se retente seule ; on peut toujours prendre une photo. |
| Hors ligne | Aucun accès réseau dans l'application. |
| Mesurer | Tout changement visible est accompagné d'une mesure processeur / mémoire avant et après. |
| Français soigné | Textes invités courts et chaleureux, apostrophe typographique (’). |

Décisions de produit déjà prises (merci de ne pas les rouvrir sans raison nouvelle) :
pas de partage en ligne, un seul format de tirage (10×15, une photo), pas de fond vert,
pas de vidéo d'attraction en boucle. Le détail est dans la feuille de route, § 7.

## Mettre en place l'environnement

```sh
cargo build --release
cargo run --release -- --demo --windowed                         # sans caméra
BOOTHSOFT_SYNTHETIC_CAMERA=guests cargo run --release -- --windowed   # caméra simulée
```

Sous Linux, installez les paquets de développement pour V4L2, X11 / Wayland et OpenGL.

## Avant d'envoyer une contribution

```sh
cargo test --release
cargo clippy --release --all-targets -- -D warnings -D dead_code
```

Les deux doivent passer sans rien afficher. Si votre changement touche l'interface ou
les performances :

```sh
Xvfb :99 -screen 0 1280x800x24 &
python3 scripts/perf/monitor.py target/release/boothsoft scripts/perf/steps-flow.json /tmp/mesure
```

Joignez à votre demande le tableau produit (avant / après) et les captures d'écran, et
reportez les mesures dans [`docs/GUEST_UI.md`](docs/GUEST_UI.md).

## Demandes de fusion

- Une demande = un sujet. Les refontes mêlées à une correction sont difficiles à relire.
- Décrivez ce qui change pour l'invité ou pour l'exploitant, pas seulement le code.
- Dites clairement ce que vous n'avez **pas** pu vérifier (Windows, vraie imprimante…).
- Ajoutez des tests pour toute logique nouvelle (formats de fichiers, calculs, cas d'erreur).
- Mettez à jour la documentation concernée.

## Ce qui ne doit jamais entrer dans le dépôt

- Des photos d'invités ou toute donnée personnelle. Utilisez `--demo` ou la caméra simulée.
- Des marques, logos ou thèmes d'entreprise. Un seul thème est livré : Party.
- Des kits de fabricants ou tout fichier sous licence non libre
  (voir [`docs/EXTENSIONS.md`](docs/EXTENSIONS.md)).
- Des secrets, des chemins absolus de votre machine.

## Licence des contributions

En proposant une contribution, vous acceptez qu'elle soit distribuée sous la licence
du projet, [CPAL-1.0](LICENSE), et vous ajoutez l'en-tête `SPDX-License-Identifier:
CPAL-1.0` aux nouveaux fichiers source. Le crédit « BoothSoft, créé par Dôm'Booth »
affiché à l'écran (`boothsoft::ATTRIBUTION`) ne doit pas être retiré ni masqué.
