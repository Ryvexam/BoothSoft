# Spécification — Règles communes de la maintenance (chantier M1)

Statut : **à développer**. Ces règles s'appliquent à toute la maintenance : réglages,
planning des événements, éditeur de thèmes (`THEME_EDITOR.md`), cadres des événements
(`EVENT_FRAMES.md`). Les deux autres spécifications y renvoient ; développe ce socle
**avant** elles.

## 1. Vocabulaire — trois notions à ne jamais confondre

| Notion | Ce que c'est | Combien | Où c'est réglé |
| --- | --- | --- | --- |
| **Thème** | L'habillage de la borne aux couleurs de **l'entreprise** qui l'exploite : couleurs, logo, écran d'accueil, textes des boutons. Il ne dit rien de l'événement. | Très peu : le thème générique festif « Party », seul thème livré, et le thème de la marque de l'exploitant, qu'il crée ou importe lui-même (privé). Un thème change rarement. | Maintenance → Thème |
| **Événement** | Une prestation chez un client : un nom, une date, des horaires, un nombre de tirages, **son cadre**. Mariage, anniversaire, séminaire… tous utilisent le même thème. | Un par prestation. | Maintenance → Événements |
| **Cadre** | L'image posée sur la photo imprimée (10×15). **Il appartient à l'événement**, pas au thème : chaque événement a le sien (prénoms des mariés, date, logo du client…). | Un par événement, choisi dans la bibliothèque de cadres ou créé pour l'occasion. | Maintenance → Événements → (un événement) → Cadre, et Maintenance → Cadres |

Conséquences pour le code (détail dans `EVENT_FRAMES.md`) :

- Il n'existe **pas** de thème « Mariage » ou « Anniversaire ». Ces deux thèmes livrés
  aujourd'hui sont à retirer ; leurs cadres deviennent des modèles de la bibliothèque.
- Un thème ne contient plus de cadre choisi par l'utilisateur. Il garde un seul fichier
  `assets/frame.png` de **secours**, utilisé uniquement si un événement n'a aucun cadre.
- Le tirage : la photo remplit **toute** la surface du tirage, le cadre est posé
  par-dessus (comportement actuel de `compositor::compose_owned`, à conserver).

## 2. Quitter sans avoir enregistré

### 2.1 Principe

**Rien n'est jamais enregistré ni perdu en silence.** Dès qu'une action ferait
disparaître des modifications non enregistrées, une fenêtre de confirmation s'affiche.
Cela remplace le comportement actuel de « Retour aux photos », qui enregistre d'office
(`admin.rs::leave_admin`).

### 2.2 Ce qui compte comme « modifications non enregistrées »

| Zone | Détection |
| --- | --- |
| Réglages et planning | `Kiosk::admin_dirty()` : `Settings` ≠ copie enregistrée (`admin_saved`) |
| Tirages prévus (quota) | `quota_draft` ≠ valeur enregistrée de l'événement |
| Éditeur de thème | `Editor::config` ≠ `Editor::saved`, ou une image importée non enregistrée |
| Éditeur de cadre | Paramètres ou image ≠ version enregistrée |
| Champ « Nouveau code » rempli | Non : simple saisie, effacée sans question |

Une fonction unique `Kiosk::unsaved() -> Vec<UnsavedItem>` renvoie la liste (libellés
lisibles : « Réglages », « Planning des événements », « Thème “Party” »,
« Cadre “Léa & Hugo” », « Tirages prévus »).

### 2.3 Actions qui déclenchent la fenêtre

| Action | Déclenche si… |
| --- | --- |
| « Retour aux photos » | quelque chose est non enregistré |
| « Fermer BoothSoft » | idem |
| « Verrouiller maintenant » | idem |
| « ← Thèmes » / « ← Cadres » / « ← Événements » (sortie d'un éditeur) | l'éditeur concerné est modifié |
| Ouvrir un autre thème ou un autre cadre depuis un éditeur modifié | l'éditeur est modifié |
| « Annuler » dans la barre du bas | toujours (c'est un abandon volontaire) : fenêtre réduite, voir 2.5 |
| Changer de page dans le menu de la maintenance | **non** pour les réglages (ils restent en mémoire d'une page à l'autre, la barre du bas le rappelle) ; **oui** si un éditeur de thème ou de cadre est ouvert et modifié |
| Verrouillage automatique après inactivité (§ 4) | jamais de fenêtre : voir § 4 |

### 2.4 La fenêtre

Fenêtre centrée, fond assombri (`ui::appear`, 200 ms), largeur 560 px, cibles ≥ 56 px.
Un toucher sur le fond assombri équivaut à « Rester ».

```
┌──────────────────────────────────────────────────────────┐
│                                                          │
│   Quitter sans enregistrer ?                             │
│                                                          │
│   Vous avez des modifications non enregistrées :         │
│     • Planning des événements                            │
│     • Thème « Party »                                    │
│                                                          │
│   ┌────────────────────────────────────────────────┐     │
│   │        Enregistrer et quitter                  │     │  ← bouton principal
│   └────────────────────────────────────────────────┘     │
│   ┌────────────────────────────────────────────────┐     │
│   │        Quitter sans enregistrer                │     │  ← bouton rouge, contour
│   └────────────────────────────────────────────────┘     │
│   ┌────────────────────────────────────────────────┐     │
│   │        Rester sur cette page                   │     │  ← bouton neutre
│   └────────────────────────────────────────────────┘     │
│                                                          │
└──────────────────────────────────────────────────────────┘
```

| Élément | Texte exact |
| --- | --- |
| Titre | « Quitter sans enregistrer ? » |
| Phrase | « Vous avez des modifications non enregistrées : » + liste à puces (5 lignes au plus, puis « et N autres ») |
| Bouton 1 (principal) | « Enregistrer et quitter » |
| Bouton 2 (danger) | « Quitter sans enregistrer » |
| Bouton 3 (neutre) | « Rester sur cette page » |

Le verbe s'adapte à l'action d'origine : « Enregistrer et fermer BoothSoft » /
« Fermer sans enregistrer » pour la fermeture ; « Enregistrer et verrouiller » /
« Verrouiller sans enregistrer » ; « Enregistrer et ouvrir l'autre thème » /
« Ouvrir sans enregistrer ».

### 2.5 Comportement des boutons

| Bouton | Effet |
| --- | --- |
| Enregistrer et quitter | Enregistre **tout** ce qui est listé, dans l'ordre : éditeurs ouverts, puis réglages. Si un enregistrement échoue : la fenêtre se ferme, l'action d'origine est annulée, le message d'erreur s'affiche et l'on reste sur la page concernée. Si tout réussit : l'action d'origine est exécutée. |
| Quitter sans enregistrer | Rétablit les réglages enregistrés (`admin_saved`), abandonne les brouillons d'éditeur (suppression du dossier provisoire), puis exécute l'action d'origine. |
| Rester sur cette page | Ferme la fenêtre, rien d'autre. |

Pendant un enregistrement lent (éditeur) : les trois boutons sont désactivés et le
premier affiche « Enregistrement… ».

**« Annuler » de la barre du bas** : fenêtre réduite à deux boutons —
« Abandonner les modifications ? » · « Oui, abandonner » (danger) · « Continuer à modifier ».

### 2.6 Rappel permanent

La barre sombre du bas « Modifications non enregistrées · Annuler · Enregistrer »
(déjà en place pour les réglages) s'affiche de la même façon dans les éditeurs.
Un point de couleur d'accent apparaît dans le menu à côté de chaque page qui contient
des modifications.

### 2.7 Réalisation

- `enum PendingLeave { BackToPhotos, CloseApp, Lock, CloseEditor, OpenOther(…), Discard }`
  rangé dans `Kiosk::pending_leave: Option<PendingLeave>`.
- Chaque action concernée appelle `self.request_leave(action)` : si `unsaved()` est
  vide, l'action s'exécute tout de suite ; sinon `pending_leave = Some(action)`.
- `render_admin` dessine la fenêtre en dernier quand `pending_leave` est `Some`, et
  neutralise les touchers du reste de l'écran (zone `interact` plein écran dessous).
- Fichier : `src/kiosk/admin/leave.rs` (nouveau).
- Tests : pour chaque action × chaque bouton, l'état final attendu (réglages, brouillon,
  écran courant).

## 3. Historique : trois sauvegardes

### 3.1 Principe

À chaque enregistrement d'un **thème**, d'un **cadre** ou des **réglages (planning
compris)**, la version précédente est conservée. On garde les **3 dernières** versions
de chaque objet ; la plus ancienne est supprimée à la quatrième.

### 3.2 Stockage

```
BoothSoft/backups/
├── settings/
│   ├── 2026-11-14T18-02-11.json
│   ├── 2026-11-12T09-40-03.json
│   └── 2026-11-10T16-21-47.json
├── themes/<id>/
│   └── 2026-11-14T18-02-11/        (copie complète du dossier du thème)
└── frames/<id>/
    └── 2026-11-14T18-02-11/        (copie complète du dossier du cadre)
```

- Nom = date et heure locales de l'enregistrement qui a **remplacé** cette version.
- Une sauvegarde n'est créée que si le contenu a réellement changé.
- Les réglages : le code PIN haché fait partie du fichier ; restaurer d'anciens
  réglages **ne restaure pas** l'ancien code (le champ `admin_pin_hash` actuel est conservé).
- Création de la sauvegarde = étape de l'enregistrement atomique : l'ancien dossier,
  au lieu d'être supprimé, est déplacé dans `backups/` ; puis élagage à 3.
- Supprimer un thème ou un cadre déplace aussi sa dernière version dans `backups/`
  (récupérable tant qu'elle n'est pas élaguée) et garde ses sauvegardes 30 jours.
- Module `src/backups.rs` : `push(kind, id, source_dir_or_file)`, `list(kind, id) ->
  Vec<Backup { stamp, size }>`, `restore(kind, id, stamp)`, `prune(kind, id, keep = 3)`.
  Tout se fait dans un fil de travail (copies de fichiers).

### 3.3 Interface

Dans l'éditeur de thème, l'éditeur de cadre et la page Données (pour les réglages),
une carte « Historique » :

```
┌ Historique ───────────────────────────────────────────────┐
│ Version actuelle          enregistrée le 14 nov. à 18:02  │
│ ─────────────────────────────────────────────────────────│
│ 12 nov. 2026 à 09:40                       [ Restaurer ]  │
│ 10 nov. 2026 à 16:21                       [ Restaurer ]  │
│ 3 nov. 2026 à 11:05                        [ Restaurer ]  │
│ Les 3 dernières versions sont conservées.                 │
└───────────────────────────────────────────────────────────┘
```

- « Restaurer » ouvre une confirmation : « Revenir à la version du 12 nov. à 09:40 ?
  La version actuelle sera conservée dans l'historique. » · « Restaurer » · « Annuler ».
- Restaurer = enregistrer l'ancienne version comme nouvelle version courante (la
  version actuelle entre donc dans l'historique : on peut toujours revenir en arrière).
- Si l'éditeur a des modifications non enregistrées, la fenêtre du § 2 passe d'abord.
- Restaurer le thème **actif** ou le cadre de l'événement **en cours** l'applique tout de suite.

## 4. Verrouillage automatique de la maintenance

- Après 10 minutes sans toucher en maintenance : retour au live, maintenance verrouillée.
- Les modifications non enregistrées ne sont **ni enregistrées ni perdues** : les
  réglages modifiés sont écrits dans `BoothSoft/drafts/settings.json`, les brouillons
  d'éditeur restent dans leur dossier provisoire.
- À la prochaine ouverture de la maintenance (code PIN), une bannière propose :
  « Des modifications du 14 nov. à 18:40 n'ont pas été enregistrées. »
  · « Les reprendre » · « Les abandonner ».
- Même mécanisme après un redémarrage inattendu (coupure, plantage).
- Brouillons de plus de 7 jours : supprimés au démarrage.

## 5. Autres règles d'interface de la maintenance

- **Suppression** (événement, thème, cadre) : toujours une confirmation avec le nom de
  l'objet et un bouton rouge ; jamais de suppression au premier toucher. Aujourd'hui
  « Supprimer » un événement du planning agit immédiatement
  (`admin.rs::page_event`) : à corriger.
- **Messages** : en français, une phrase, sans jargon ; ce qui a été fait (« Thème
  enregistré »), ou ce qui bloque et quoi faire.
- **Opérations longues** : bouton désactivé + libellé « …en cours », jamais d'écran figé.
- **Tout au doigt** : cibles ≥ 44 px (≥ 67 px sur l'écran 15,6" en 1080p), pas de
  survol, pas de clic droit, champs de texte avec le clavier tactile
  (`TOUCH_KEYBOARD.md`).
- **Animations** : entrées de fenêtres et de pages par `ui::appear`, rien en boucle.

## 6. Effort et ordre

| Lot | Contenu | Effort |
| --- | --- | --- |
| M1.1 ✅ | `unsaved()`, `request_leave`, fenêtre de confirmation, remplacement de l'enregistrement d'office de `leave_admin`, confirmation de suppression d'un événement. **Fait le 11/11/2026**, dans `admin.rs` (pas de fichier `leave.rs` séparé) ; la fenêtre des éditeurs (thème, cadre) s'y branchera en ajoutant leurs éléments à `unsaved()`. | S |
| M1.2 | `backups.rs` + historique des réglages (page Données) | M |
| M1.3 | Verrouillage automatique et brouillons récupérables | S |

M1.1 est un prérequis de `THEME_EDITOR.md` et `EVENT_FRAMES.md`.
