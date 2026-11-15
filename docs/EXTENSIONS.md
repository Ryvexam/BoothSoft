# Extensions privées : garder chez soi ce qui ne peut pas être publié

BoothSoft est libre, mais deux choses ne peuvent pas vivre dans un dépôt public :

1. **Les kits des fabricants d'appareils photo** (par exemple le Canon EOS Digital SDK) :
   leur licence interdit de les redistribuer.
2. **La marque de l'exploitant** : son thème, son logo, ses cadres clients.

Ce document décrit comment un exploitant
ajoute ces éléments **sans modifier le dépôt public et sans rien y laisser fuiter**.

> **Statut.** Le thème privé fonctionne dès aujourd'hui (§ 2). Le point d'extension pour
> les caméras (§ 3) est **à développer** : c'est le lot A1.0 de la feuille de route.

## 1. Principe

Le dépôt public ne connaît **rien** du privé : aucune dépendance facultative vers un
chemin absent, aucune option de compilation qui casserait chez les autres. C'est le
dépôt privé qui dépend du dépôt public, jamais l'inverse.

```
┌─────────────────────────────┐        ┌──────────────────────────────────────┐
│ Dépôt public  boothsoft     │        │ Dépôt privé  (ex. ma-marque-borne)   │
│                             │        │                                      │
│  bibliothèque  boothsoft    │◀───────│  kiosk/   exécutable de la borne     │
│   · app::run(Extensions)    │ dépend │  canon/   liaison avec le kit Canon  │
│   · camera::FrameSource     │        │  vendor/  kit Canon (jamais publié)  │
│  exécutable    boothsoft    │        │  themes/  thème de la marque         │
│   (webcam, thème Party)     │        │                                      │
└─────────────────────────────┘        └──────────────────────────────────────┘
```

- Tout le monde peut compiler le dépôt public : il donne une borne complète en webcam,
  au thème Party.
- L'exploitant compile son dépôt privé : il obtient **la même borne**, plus sa caméra
  reflex et son thème.

## 2. Thème privé (fonctionne aujourd'hui)

Un thème d'utilisateur n'a pas besoin d'être dans le dépôt :

- déposez son dossier dans le dossier de données de l'application
  (`BoothSoft/themes/<identifiant>/`), ou importez son ZIP depuis la maintenance ;
- choisissez-le dans Maintenance → Thème.

Conservez la source du thème (logo, illustrations, `config.toml`) dans votre dépôt
privé, sous `themes/<identifiant>/`, et copiez-la sur chaque borne à l'installation.
Le fichier `.gitignore` du dépôt public ignore tout dossier de `themes/` autre que
`default`, pour éviter un envoi par erreur.

## 3. Caméra privée (à développer : lot A1.0)

### 3.1 Ce que le dépôt public doit offrir

1. **L'application dans la bibliothèque.** Aujourd'hui l'interface (`src/kiosk.rs` et
   `src/kiosk/`) est compilée dans l'exécutable. La déplacer dans la bibliothèque sous
   `boothsoft::app`, avec un seul point d'entrée :

   ```rust
   // src/app/mod.rs
   #[derive(Default)]
   pub struct Extensions {
       /// Sources de caméra supplémentaires, interrogées avant les webcams.
       pub cameras: Vec<Box<dyn camera::CameraProvider>>,
   }
   pub fn run(extensions: Extensions) -> eframe::Result;
   ```

   `src/main.rs` se réduit à `fn main() -> eframe::Result { boothsoft::app::run(Default::default()) }`.

2. **Un contrat de fournisseur de caméra**, à côté du `FrameSource` existant
   (`src/camera.rs`) :

   ```rust
   pub trait CameraProvider: Send {
       /// Identifiant stable, enregistré dans les réglages (ex. "canon-eos").
       fn id(&self) -> &'static str;
       /// Appareils actuellement branchés. Appelé depuis le fil de recherche d'appareils.
       fn detect(&self) -> Vec<CameraDevice>;
       /// Ouvre l'appareil. Appelé depuis le fil caméra, qui fera ensuite tous les appels.
       fn open(&self, device: &CameraDevice) -> Result<Box<dyn FrameSource>, String>;
   }
   ```

   Évolutions nécessaires de `FrameSource` pour un reflex (aujourd'hui une seule méthode
   `read(buffer, rgba)`) :

   | Méthode | Rôle | Défaut pour les webcams |
   | --- | --- | --- |
   | `read_preview(&mut self, buffer, rgba) -> Result<(u32, u32), String>` | Une image de visée | l'actuel `read` |
   | `capture(&mut self) -> Result<Captured, String>` | Déclenche et renvoie la photo : `Captured::Rgb(RgbImage)` ou `Captured::Jpeg(Vec<u8>)` (fichier de l'appareil, gardé tel quel comme original) | une image du flux |
   | `set_idle(&mut self, idle: bool)` | Veille : couper la visée, baisser le miroir | rien |
   | `keep_alive(&mut self)` | Appelé une fois par minute | rien |
   | `status(&self) -> SourceStatus` | Batterie / secteur, nom du modèle, taille de photo | vide |

   `CameraDevice` gagne un champ `provider: String` (vide = webcam) ; `Settings`
   enregistre le fournisseur avec l'index de la caméra.

3. **Aucune mention d'un fabricant** dans le dépôt public : ni en-tête, ni nom de
   fonction, ni option de compilation. Seulement le contrat ci-dessus et un fournisseur
   d'exemple pour les tests (la caméra simulée existante, transformée en fournisseur).

### 3.2 Le dépôt privé

```
ma-marque-borne/                    dépôt privé
├── Cargo.toml                      [workspace] members = ["kiosk", "canon"]
├── boothsoft/                      sous-module Git → dépôt public, à une version précise
├── canon/                          crate `boothsoft-canon`
│   ├── Cargo.toml                  dépend de `boothsoft = { path = "../boothsoft" }`
│   ├── build.rs                    lie le kit depuis ../vendor/edsdk
│   └── src/lib.rs                  liaison minimale + `pub fn provider() -> Box<dyn CameraProvider>`
├── kiosk/                          crate `ma-marque-kiosk` (l'exécutable installé sur les bornes)
│   └── src/main.rs
├── vendor/edsdk/                   kit Canon : en-têtes, bibliothèques (jamais publié)
└── themes/ma-marque/               thème de la marque
```

```rust
// kiosk/src/main.rs
fn main() -> eframe::Result {
    boothsoft::app::run(boothsoft::app::Extensions {
        cameras: vec![boothsoft_canon::provider()],
    })
}
```

```sh
# Compilation de l'exécutable des bornes
cargo build --release -p ma-marque-kiosk
```

- Les bibliothèques du kit (`EDSDK.dll`, `EdsImage.dll` sous Windows) sont copiées à côté
  de l'exécutable **sur les bornes de l'exploitant uniquement**.
- Mise à jour de BoothSoft : avancer le sous-module, recompiler, tester.
- Si une évolution du contrat public casse la crate privée, c'est à la crate privée de
  s'adapter ; le dépôt public garde un test de non-régression du contrat (fournisseur
  d'exemple).

### 3.3 Règles pour ne rien laisser fuiter

| Règle | Comment |
| --- | --- |
| Aucun fichier du kit dans le dépôt public | `.gitignore` : `/vendor/`, `/private/` ; revue de chaque demande de fusion |
| Aucun nom de marque dans le code public | Recherche automatique du nom de la marque dans l'intégration continue du dépôt privé avant toute contribution |
| Travail au bon endroit | Tout ce qui est générique (contrat, veille, reprise après débranchement, mise à l'échelle des photos) va dans le dépôt public ; seul l'appel au kit reste privé |
| Historique | Vérifier l'historique Git avant la première publication : un fichier supprimé y reste visible |

## 4. Ce qui reste public même pour un reflex

Pour que la version libre progresse elle aussi, tout ce qui n'appelle pas le kit se
développe dans le dépôt public et profite aux webcams :

- photo en pleine résolution distincte de la visée (`capture`) ;
- original conservé tel que fourni par l'appareil, décodage à échelle réduite ;
- image au format 3:2 et mise en page de l'écran en conséquence ;
- reprise automatique après débranchement, veille de l'appareil, maintien en éveil ;
- page Caméra de la maintenance (état, batterie, taille de photo).

Une alternative entièrement libre pour les reflex (libgphoto2, sous Linux et macOS)
pourra être ajoutée plus tard comme fournisseur **public**, sans rien changer au contrat.
