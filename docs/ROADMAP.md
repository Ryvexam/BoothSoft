# BoothSoft — axes d'amélioration et futures implémentations

> **Agent qui reprend le projet : commence par `AGENTS.md` à la racine.**
> Architecture existante : `docs/ARCHITECTURE.md`. Spécifications détaillées dans
> `docs/specs/` : `MAINTENANCE_PATTERNS.md` (vocabulaire thème / événement / cadre,
> « Quitter sans enregistrer ? », historique), `EVENT_FRAMES.md` (un cadre par
> événement), `THEME_EDITOR.md` (thème de l'entreprise), `TOUCH_KEYBOARD.md`.
> Les fiches de ce document servent de spécification pour les autres chantiers.

**À ne jamais confondre.** Le *thème* habille la borne aux couleurs de l'entreprise
(celle de l'exploitant) et ne change pas. L'*événement* est une prestation (nom, date, horaires,
tirages). Le *cadre* du tirage appartient à l'événement : chaque événement a le sien.
Il n'y a pas de thème « mariage » ou « anniversaire ».

**Projet open source.** BoothSoft est publié sous licence libre. Un seul thème est
livré : « Party ». Le thème de marque d'un exploitant reste chez lui, hors du dépôt
(fiche O3).

État au 11/11/2026, version 0.16. Ce document sert de référence pour les prochains
chantiers : chaque fiche donne le constat (avec le fichier concerné), la solution
retenue, le critère d'acceptation mesurable et l'effort (S ≈ ½ journée, M ≈ 1–2 jours,
L ≈ 3–5 jours, XL > 1 semaine).

Les gains chiffrés notés « estimé » ne sont pas mesurés : ils sont à confirmer avec
`scripts/perf/monitor.py` avant et après chaque chantier.

---

## 1. Cap et budgets

**Cap produit** : une borne qui s'allume et fonctionne seule, aussi simple à utiliser que
LumaBooth / dslrBooth (toucher → pose → photo → impression), mais hors ligne, plus légère,
et pilotée par un planning d'événements.

**Matériel de référence** (toute décision se prend pour cette configuration) :

| Composant | Référence | Ce que ça implique pour le logiciel |
| --- | --- | --- |
| Appareil photo | Canon EOS 2000D (reflex) | Pilotage USB par le kit Canon ; visée 1056×704 au format 3:2 ; photo jusqu'à 24 Mpx (fiche A1) |
| Imprimante | DNP QW410, sublimation thermique, 300 dpi | Tirage 4×6 en ~19 s ; rouleau à nombre de tirages fixe ; un seul format, le 10×15 (fiches Z3, C2) |
| Écran | 15,6 pouces tactile (définition à relever, 1920×1080 supposé) | Écran 16:9 pour une image 3:2 ; cibles tactiles d'au moins 12 mm (fiche E1) |
| Mini-PC | Intel Core i5, 8 Go, SSD 256 Go (génération à relever) | Budgets ci-dessous ; puce graphique intégrée utilisée pour le live |

**Budgets à ne jamais dépasser** :

| Mesure | Budget | Aujourd'hui (Linux, rendu logiciel) |
| --- | --- | --- |
| CPU au repos (tous écrans statiques) | 0 % | 0 % ✔ |
| CPU en veille (détection de présence) | ≤ 2 % | ~1 % ✔ |
| CPU live 30 img/s avec GPU | ≤ 15 % d'un cœur | ~15 % (estimé, jamais mesuré sur le PC cible) |
| CPU live 30 img/s sans GPU | ≤ 50 % d'un cœur | 48 % scène animée, 19 % scène fixe ✔ |
| RAM au repos | ≤ 150 Mo | ~140 Mo ✔ |
| RAM pic (rafale + choix) | ≤ 200 Mo avec webcam, ≤ 260 Mo avec le 2000D | ~165 Mo (webcam simulée) |
| Toucher → réaction visible | ≤ 50 ms | non mesuré |
| Fin du décompte → photo affichée | ≤ 1,5 s avec webcam, ≤ 2,5 s avec le 2000D | non mesuré sur le PC cible |
| Allumage du PC → live prêt | ≤ 45 s | non mesuré |

**Principe directeur : zéro intervention.** Personne de chez l'exploitant n'est sur place ni
connecté à distance pendant un événement. La borne doit se débrouiller seule : détecter
chaque panne, la réparer elle-même si c'est logiciel, et sinon continuer en mode réduit
en guidant la personne présente (le client) pour le seul geste physique nécessaire.
Tout le § 4 (fiches Z) en découle et passe avant les nouvelles fonctions.

**Règles permanentes** (déjà appliquées, à conserver) :

1. **Pensé pour le doigt.** Chaque toucher répond dans l'image suivante, chaque
   changement d'état se voit par un mouvement court (≤ 250 ms) : boutons qui s'enfoncent,
   sélecteurs qui glissent, écrans qui arrivent, photos qui suivent le doigt. En
   revanche rien ne tourne en boucle : un écran au repos ne se redessine jamais (0 % CPU).
2. Rien de lent sur le fil d'interface : pas de décodage d'image, pas de disque, pas de
   sous-processus, pas de hachage. Tout passe par un fil de travail qui réveille l'interface.
3. Une seule texture pleine taille en mémoire à la fois ; les listes utilisent des vignettes.
4. Chaque chantier se termine par une mesure avant / après, ajoutée à `docs/GUEST_UI.md`.
5. Aucun écran sans issue : tout écran revient seul au live après un délai, toute erreur
   se retente seule. Jamais de bouton obligatoire, jamais de message qui demande d'« appeler l'équipe ».
6. On peut toujours prendre une photo : une panne d'imprimante, de cadre, de thème ou de
   quota ne bloque jamais la prise de vue ni l'enregistrement.

---

## 2. Écart avec LumaBooth / dslrBooth

Sources : page des fonctionnalités de LumaBooth pour Windows (dslrBooth), novembre 2026 ;
fiche de configuration du Canon EOS 2000D chez Dragonframe (visée USB 1056×704, réglages) ;
fiches DNP QW410 (300 dpi, 190 tirages 4×6 par heure, 2×6 sur papier 4×6 perforé, micrologiciel 1.10).

| Domaine | LumaBooth / dslrBooth | BoothSoft | Verdict |
| --- | --- | --- | --- |
| Appareils | Reflex et hybrides Canon / Nikon / Sony, GoPro, webcam | Webcam seulement | **Écart majeur** : Canon 2000D (fiche A1) |
| Qualité photo | Pleine résolution du capteur | Image du flux vidéo 1280×720 | **Écart majeur** (fiche A1) |
| Modes | Photo, GIF, boomerang, vidéo, 360 | Photo, rafale avec choix | Écart (fiche C1) |
| Modèles d'impression | Éditeur de modèles, plusieurs photos par tirage, 4×6 et bandes 2×6 | Un cadre PNG par thème, 1 photo par tirage 10×15 | Choix assumé (fiche C2) |
| Effets | Filtres, LUT, fond vert, stickers, lissage de peau | 6 filtres en direct | Suffisant pour démarrer |
| Partage | E-mail, SMS, QR (y compris hors ligne), microsite | Aucun (choix assumé) | Hors périmètre (§ 7) |
| Impression | Toute imprimante, envoi automatique, paiement | Impression directe, quota par événement, confirmation manuelle | Écart (fiche B3) |
| Écran d'accueil | Vidéo d'attraction en boucle, thèmes | Illustration fixe par thème | À notre avantage (0 % CPU) |
| Événements | Création dans l'appli ou tableau de bord web, synchro cloud | Planning local avec horaires, dossier et quota par événement | Équivalent hors ligne ; manque thème par événement (fiche C3) |
| Suivi | Statistiques, appli de supervision à distance | Compteurs dans la maintenance | Écart mineur (fiche C5) |
| Démarrage borne | Non décrit | Lancement au démarrage, plein écran, veille | À notre avantage |
| Ressources | Windows 10/11, non communiqué | 140 Mo, 0 % au repos | À notre avantage |

---

## 3. Vue d'ensemble des chantiers

Ordre recommandé : d'abord le Canon 2000D (tout le reste se mesure avec lui), puis ce
qui peut faire échouer une soirée, puis la fluidité, puis les fonctions.

| # | Chantier | Priorité | Effort | Gain principal |
| --- | --- | --- | --- | --- |
| Z1 | ✅ Gardien : relance automatique, mode sans échec, redémarrage nocturne (reste : réglages Windows) | **P0** | M | L'application ne reste jamais plantée |
| Z2 | ◐ Caméra auto-réparatrice (reconnexion : fait ; repli webcam, exposition : à faire) | **P0** | M | Toujours une image |
| Z3 | Impression auto-gérée (file relancée, mode sans tirage, rattrapage) | **P0** | L | Aucune confirmation ni déblocage manuel |
| Z4 | Guidage à l'écran de la personne sur place | **P0** | M | Le client règle seul papier, câble, allumage |
| Z5 | Données auto-protégées (réglages, disque, horloge, coupure de courant) | **P0** | M | Rien ne se perd, rien ne se bloque |
| Z6 | Contrôle avant événement et bilan automatique | P1 | S | Problèmes vus avant les invités |
| E1 | Mise en page pour l'écran 15,6" (image 3:2 entière, commandes sur les côtés) | P1 | M | Ce qu'on voit est ce qui s'imprime |
| V1 | Mesures sur le vrai PC de la borne | P0 | S | Sait-on vraiment où on en est |
| V2 | Valider la version Windows (OpenGL, démarrage auto, impression) | P0 | M | La borne fonctionne sur sa machine |
| B1 | ✅ Caméra débranchée / rebranchée, reprise automatique | P0 | M | Pas d'intervention pendant la soirée |
| B2 | ✅ Journal sur disque et écran d'erreur utile (reste : export ZIP des journaux) | P0 | S | Diagnostic après coup |
| B4 | ✅ Garde disque plein et nettoyage des anciens événements (reste : suppression automatique après N jours) | P0 | S | 256 Go ne se remplissent pas en silence |
| A1 | **Canon EOS 2000D en USB** : visée, déclenchement, photo | **P0** | L | L'appareil de la borne fonctionne ; tirages nets |
| A2 | Webcam : photo en pleine résolution (secours) | P3 | M | Qualité en mode de secours |
| B3 | Suivi réel des impressions (file d'attente du système) | P1 | L | Plus de confirmation manuelle |
| P1 | Décodage du live sans conversion couleur sur le CPU | P1 | M | −30 à −40 % du coût de décodage (estimé) |
| P2 | Plus rien de lent sur le fil d'interface | P1 | M | Zéro saccade au toucher |
| P3 | Index des séances par événement | P1 | M | RAM et démarrage constants dans le temps |
| U1 | Parcours invité : sons, reprise de rafale, réimpression | P2 | M | Simplicité type dslrBooth |
| C2 | Tirage 10×15 à la taille exacte de l'imprimante | P2 | S | Netteté maximale, cadre jamais rogné |
| U2 | Gestes tactiles : suite (balayage des filtres, retour haptique visuel, inertie) | P2 | M | Fluidité « tablette » partout |
| M1 | Règles communes de la maintenance : « Quitter sans enregistrer ? », historique à 3 versions, brouillons (`docs/specs/MAINTENANCE_PATTERNS.md`) | P1 | M (3 lots) | Rien n'est perdu ni enregistré en silence |
| F1 | **Un cadre par événement** : bibliothèque de cadres, choix dans chaque événement, cadres fabriqués avec nom et date, paysage et portrait (`docs/specs/EVENT_FRAMES.md`) | P1 | XL (5 lots) | Chaque prestation a son tirage personnalisé, appliqué seul |
| K1 | Clavier tactile de la maintenance (`docs/specs/TOUCH_KEYBOARD.md`) | P1 | M | Saisir un nom ou un texte sur la borne, sans clavier |
| T1 | Éditeur du thème de l'entreprise : couleurs, logo, accueil, textes (`docs/specs/THEME_EDITOR.md`) | P2 | L (4 lots) | Adapter l'habillage sans autre logiciel |
| G1 | ✅ **Livre d'or vidéo** (à débloquer, fait : G1.1–G1.5) : message vidéo avec le son, 30 s au plus, pour l'organisateur (`docs/specs/GUEST_BOOK.md`) | P2 | XL (5 lots) | Option vendable, souvenir qui manque aux photos |
| L1 | ◐ **Système de langues** : anglais par défaut et français faits ; espagnol, chinois (`docs/specs/LANGUAGES.md`) | P2 | XL (5 lots) | Projet utilisable hors de France |
| C3 | Réglages par événement (tirages prévus, exemplaires, rafale, limite de séances) | P2 | M | « La borne s'allume, tout est prêt » |
| C1 | GIF / boomerang depuis la rafale | P3 | M | Parité de modes |
| C5 | Bilan d'événement et export USB | P3 | S | Livraison au client |
| O1 | Découpage du code, tests d'interface, intégration continue | P3 | M | Vitesse de développement |
| O3 | ✅ **Publication open source** : aucune marque dans le dépôt, un seul thème livré (« Party »), licence, README | **P1** | S | Dépôt publiable sans donnée privée |
| O2 | Installeur Windows signé et durcissement de la borne | P3 | M | Mise en service en 10 minutes |

---

## 4. Fiches détaillées

### Tableau des pannes : ce que la borne fait seule

| Panne | Détection | Réaction automatique | Ce que voient les invités |
| --- | --- | --- | --- |
| L'application plante ou se fige | Gardien : plus de signe de vie depuis 15 s | Arrêt forcé, relance, retour au live en < 10 s | Écran noir quelques secondes |
| 3 plantages en 5 minutes | Gardien | Mode sans échec : thème par défaut, rendu de secours, webcam | La borne marche, habillage simplifié |
| Fenêtre Windows par-dessus (mise à jour, alerte) | Perte du premier plan | Reprise du premier plan ; notifications coupées à l'installation | Rien |
| Canon débranché, éteint, batterie vide | Erreur de lecture ou plus d'image depuis 3 s | Nouvelle connexion toutes les 2 s ; après 20 s, bascule sur la webcam de secours ; retour au Canon dès qu'il revient | « Un instant… » puis live |
| Visée figée | Même image 3 s de suite | Redémarrage de la session de l'appareil | Rien |
| Image trop sombre / cache d'objectif | Luminosité moyenne de la visée | Montée automatique de la sensibilité (dans des bornes fixées) ; sinon guidage | Rien, ou carte de guidage |
| Photo non reçue après déclenchement | Délai de 6 s dépassé | Second déclenchement automatique, puis image de visée à défaut | « Encore une ! » |
| Imprimante éteinte / débranchée | État du système | Bouton Imprimer remplacé par « Tirages bientôt de retour » ; tirages demandés mis en attente | Photos toujours possibles |
| Plus de papier / ruban | État de la file d'impression | Mise en attente + carte de guidage « Rechargez le papier » | Carte illustrée |
| File d'impression en pause ou tirage bloqué | Tirage sans progrès depuis 90 s | Reprise de la file ; annulation et renvoi du tirage (une seule fois, sans doublon) | Rien |
| Imprimante revenue | État du système | Impression des tirages en attente de moins de 15 min ; les plus vieux sont abandonnés et notés | Tirages qui sortent |
| Nombre de tirages prévu atteint | Compteur de l'événement | Bouton Imprimer retiré, message doux | « Les tirages sont terminés, vos photos sont gardées » |
| Disque presque plein | Contrôle toutes les 50 séances | Suppression des fichiers recalculables puis des événements les plus anciens déjà exportés | Rien |
| Réglages ou planning illisibles | Lecture au démarrage | Retour à la dernière copie valide (3 copies gardées) | Rien |
| Thème ou cadre manquant | Chargement | Thème par défaut ; tirage sans cadre plutôt que pas de tirage | Habillage simplifié |
| Coupure de courant | Redémarrage | PC rallumé seul (réglage du BIOS), session ouverte, application lancée, séance interrompue nettoyée, tirages en cours vérifiés | Retour au live |
| Horloge du PC fausse (pile, pas de réseau) | Date antérieure à la dernière date connue | Reprise de la dernière heure connue + temps écoulé ; planning conservé | Rien |
| Événement non prévu au planning | Aucun créneau en cours | Dossier automatique « hors planning » par jour : les photos ne se mélangent jamais | Rien |
| Écran tactile qui « colle » | Appui de plus de 10 s | Appui ignoré | Rien |
| Fuite mémoire, pilote graphique fatigué | Heure creuse (5 h) et borne inactive | Redémarrage préventif de l'application ; du PC une fois par semaine | Rien |

### Z1 — Gardien de l'application (P0, M)

> **Statut (11/11/2026) : fait en grande partie.** `src/bin/boothsoft-guard.rs` +
> `src/watchdog.rs` : lancement de l'app, battement toutes les 5 s émis seulement si le
> fil d'interface dessine encore, relance après plantage ou 30 s sans battement (60 s
> au premier démarrage), délai 1 / 3 / 10 s, mode sans échec (`--safe-mode` : thème
> livré, live sans OpenGL direct) après 3 échecs en 5 min, retour au mode normal après
> 10 min de fonctionnement, arrêt du gardien quand l'app se ferme normalement,
> redémarrage nocturne de l'app à 5 h (veille, hors créneau, après 1 h de
> fonctionnement, code de sortie 75), journalisation de tout. Le démarrage automatique
> lance le gardien s'il est à côté de l'exécutable. Vérifié sous Linux : `kill -9`
> → relance en 1 s ; processus gelé (`SIGSTOP`) → tué et relancé, puis mode sans échec.
> **Reste à faire** : premier plan forcé, réglages Windows à l'installation
> (veille, notifications, mises à jour), redémarrage hebdomadaire du PC, essais sous Windows.

**Constat.** Si BoothSoft s'arrête ou se fige, l'écran reste sur le bureau Windows
jusqu'à ce que quelqu'un intervienne. Rien ne surveille l'application.

**À faire.**
- Petit programme séparé `boothsoft-guard` (quelques centaines de Ko, 0 % CPU) : c'est
  lui qui est lancé au démarrage du PC et qui lance BoothSoft.
- Signe de vie : BoothSoft écrit un battement toutes les 5 s depuis le fil d'interface
  (par un tube, pas sur le disque). Sans battement pendant 15 s : arrêt forcé et relance.
- Reprise : au lancement, l'application va droit au live de l'événement en cours ;
  la séance interrompue est fermée proprement, les photos déjà prises sont gardées.
- Mode sans échec après 3 relances en 5 minutes : thème par défaut, rendu de secours,
  webcam, journal détaillé. Retour au mode normal au prochain démarrage réussi de 10 minutes.
- Redémarrage préventif de l'application à 5 h si la borne est en veille ; redémarrage
  du PC une fois par semaine hors créneau d'événement.
- Premier plan : fenêtre toujours au-dessus, reprise immédiate si une autre fenêtre passe devant.
- À l'installation, le gardien règle Windows une fois : veille et écran de veille
  désactivés, notifications coupées, mises à jour reportées hors des créneaux du planning.

**Acceptation.** Tuer le processus pendant une séance : live de retour en < 10 s, aucune
photo perdue. Figer l'application (test intégré) : même résultat. 0 % CPU pour le gardien.

### Z2 — Caméra auto-réparatrice (P0, M) — remplace B1

> **Statut (15/11/2026) : reconnexion faite.** `src/camera.rs` : après 15 lectures
> ratées (≈ 0,5 s) ou un échec d'ouverture, la caméra est déclarée perdue
> (`CameraEvent::Lost`) ; le fil caméra retente seul toutes les 2 s pendant 30 s, puis
> toutes les 5 s, sans réveiller l'interface ; `CameraEvent::Connected` ramène au live.
> L'écran « L'appareil photo fait une pause » n'a plus de bouton : il affiche
> « Reconnexion… N s » (1 rafraîchissement par seconde, ≈ 2 % CPU mesuré). Une séance
> en cours est abandonnée proprement (photos déjà prises gardées), un message du livre
> d'or en cours aussi. Test : `BOOTHSOFT_SYNTHETIC_UNPLUG=5-14` débranche la caméra
> simulée entre 5 et 14 s : retour au live seul en 11 s. **Reste** : repli webcam,
> chien de garde d'image figée, exposition, mise au point (avec le Canon, A1).

**À faire** (en plus de la fiche A1).
- Reconnexion sans fin : toutes les 2 s, puis 5 s, sans réveiller l'interface.
- Repli : après 20 s sans Canon, bascule sur la webcam de secours si elle existe ;
  retour au Canon dès qu'il répond, entre deux séances seulement.
- Chien de garde de la visée : même image pendant 3 s → session de l'appareil relancée.
- Photo manquante : second déclenchement, puis à défaut la dernière image de visée
  (une photo moins belle vaut mieux qu'un écran d'erreur).
- Exposition : mesure de la luminosité de la visée (déjà calculée pour la détection de
  mouvement, coût nul) ; correction automatique de la sensibilité entre deux bornes
  réglées en maintenance. Image noire persistante → carte de guidage « cache d'objectif ».
- Mise au point : refaite automatiquement toutes les 30 minutes quand personne n'est devant.
- L'écran « L'appareil photo fait une pause » n'a plus de bouton : il affiche une
  reprise automatique et disparaît seul.

**Acceptation.** Débrancher le câble, éteindre l'appareil, retirer l'alimentation :
dans les trois cas la borne revient au live sans aucun toucher.

### Z3 — Impression auto-gérée (P0, L) — complète B3

**À faire.**
- Suivi de la file du système (fiche B3) : plus aucune confirmation manuelle.
- États gérés seuls : file en pause → reprise ; tirage bloqué 90 s → annulé et renvoyé
  une fois ; imprimante hors ligne → nouvelle tentative toutes les 10 s.
- **Mode sans tirage** : dès que l'imprimante ne peut pas imprimer, le bouton Imprimer
  devient « Tirages bientôt de retour » ; les demandes sont mises en attente (15 min max)
  et sortent seules au retour de l'imprimante.
- Changement d'imprimante : si l'imprimante choisie disparaît et qu'une autre du même
  modèle apparaît (autre port USB), elle est adoptée automatiquement.
- Fin des tirages prévus : message doux, prise de vue inchangée.
- Compteur de papier estimé (tirages depuis le dernier rechargement, capacité du
  rouleau réglée en maintenance) : carte de guidage affichée *avant* la panne, à 10 tirages de la fin.

**Acceptation.** Éteindre l'imprimante en pleine soirée simulée, la rallumer 5 minutes
après : aucun toucher en maintenance, tirages en attente imprimés, aucun doublon.

**Spécificités DNP QW410** (à confirmer imprimante branchée) :
- **Rouleau = compteur.** Un rouleau 4×6 donne un nombre fixe de tirages (150 d'après
  la fiche du consommable, à vérifier sur la boîte). Le nombre de tirages restants se
  lit auprès du pilote DNP ; à défaut, la borne le compte elle-même depuis le dernier
  rechargement. Il s'affiche en maintenance et alimente le « tirages restants » de
  l'écran de veille : le quota de l'événement ne peut jamais dépasser ce qu'il reste
  dans le rouleau sans prévenir (carte « Changez le rouleau » à 10 tirages de la fin).
- **Chaque tirage raté consomme du ruban.** Le renvoi automatique ne se fait que si
  l'imprimante déclare que le tirage n'a pas commencé ; sinon il est noté « à vérifier »
  et n'est pas réimprimé.
- **Taille exacte.** Lire la zone imprimable auprès du pilote et composer l'image à
  cette taille exacte (aujourd'hui 1800×1200 étiré par le pilote) : netteté maximale,
  cadre du thème jamais rogné. Prévoir la marge de débord dans les cadres des thèmes.
- **Durée.** ~19 s par tirage 4×6 : l'écran de remerciement annonce l'attente réelle
  (« 3 tirages, environ 1 minute ») et la borne reste utilisable pendant l'impression.
- **Mise à jour.** Micrologiciel 1.10 et pilote récent pour les formats supplémentaires
  (4×3, 4×4,5) ; à faire une fois à la mise en service.

### Z4 — Guidage de la personne sur place (P0, M)

**Constat.** Certaines pannes demandent un geste physique (papier, câble, bouton
marche). Aujourd'hui l'écran dit d'« appeler l'équipe ».

**À faire.**
- Cartes de guidage illustrées, affichées dans un coin du live sans bloquer les photos :
  « Rechargez le papier », « Allumez l'imprimante », « Vérifiez le câble de l'appareil
  photo », « Retirez le cache de l'objectif », « Branchez l'appareil sur le secteur ».
  Trois étapes maximum, dessins fournis par le thème, texte simple.
- La carte disparaît seule dès que le problème est réglé (détection automatique, pas
  de bouton « C'est fait »).
- Fiche plastifiée pour le client reprenant les mêmes dessins (hors logiciel).
- Aucun code PIN, aucune maintenance : le client ne touche jamais aux réglages.

**Acceptation.** Une personne qui n'a jamais vu la borne remet du papier en suivant
la carte, sans aide, et la carte disparaît.

### Z5 — Données auto-protégées (P0, M) — englobe B4

**À faire.**
- Réglages, planning, journal des tirages : 3 copies tournantes ; retour automatique à
  la dernière copie lisible.
- Disque : contrôle régulier ; libération automatique dans cet ordre : versions « cadre »
  des événements terminés (recalculables), puis événements exportés les plus anciens.
  Jamais l'événement en cours.
- Horloge : dernière heure connue enregistrée à chaque minute ; si l'horloge du PC
  recule, la borne continue sur la dernière heure connue. Le planning ne saute pas.
- Dossier « hors planning » daté quand aucun créneau n'est en cours.
- Coupure de courant : écritures déjà sûres (`durable_file.rs`) ; au redémarrage,
  nettoyage de la séance interrompue et vérification des tirages en cours.
- Guide de mise en service : « rallumage automatique après coupure » dans le BIOS,
  ouverture de session automatique.

**Acceptation.** Couper le courant pendant une rafale puis rebrancher : la borne revient
seule au live du bon événement, les photos déjà enregistrées sont intactes.

### Z6 — Contrôle avant événement et bilan automatique (P1, S)

- 30 minutes avant chaque créneau du planning (ou à l'allumage) : contrôle complet
  (appareil photo, mise au point, exposition, imprimante, papier estimé, disque, thème,
  cadre), corrections automatiques, puis écran d'accueil avec un voyant discret
  vert / orange dans un coin et, si besoin, la carte de guidage correspondante.
- À la fin du créneau : bilan écrit dans le dossier de l'événement (séances, photos,
  tirages, pannes survenues et réparées, durée d'indisponibilité), planche-contact,
  dossier prêt à copier. Aucune action demandée.
- Le programme `boothsoft-doctor` existant sert de base au contrôle.

### E1 — Mise en page pour l'écran 15,6 pouces (P1, M)

**Constat.** L'interface est dessinée pour une image qui remplit l'écran. Avec le
Canon, l'image est au format 3:2 et l'écran au format 16:9 : soit on rogne le haut et le
bas (les invités ne voient pas ce qui sera imprimé), soit on laisse des bandes vides.

**À faire.**
- Live : image 3:2 entière sur toute la hauteur (1620×1080 sur un écran 1920×1080),
  et deux colonnes latérales de 150 px pour les commandes : filtres d'un côté,
  « Une photo / Rafale » et déclencheur de l'autre. Plus rien ne recouvre les visages.
- Moins de pixels à dessiner sous l'image : les commandes ne se superposent plus au
  live (gain de rendu, surtout sans puce graphique).
- Cibles tactiles : au moins 12 mm de côté (≈ 67 px sur un 15,6" en 1080p), 8 mm
  d'écart ; vérifier le déclencheur, les − / + et les vignettes de la rafale.
- Écran incliné ou en portrait : prévoir la rotation dans les réglages (à décider
  selon le montage de la borne).
- Luminosité : fond blanc plein écran utilisé comme éclairage d'appoint juste avant la
  photo si aucun flash n'est monté.

**Acceptation.** L'image affichée et le tirage ont exactement le même cadrage ;
tous les boutons se touchent du premier coup avec le pouce.

### V1 — Mesures sur le vrai PC de la borne (P0, S)

**Constat.** Toutes les mesures viennent d'une machine cloud Linux en rendu logiciel
avec une caméra simulée. Rien n'a été mesuré sur Windows, sur l'i5 de la borne, ni avec
une vraie caméra USB.

**À faire.**
- Porter `scripts/perf/monitor.py` sur Windows (lecture CPU/RAM par `psutil`, clics par
  `pyautogui` ou un mode de test intégré `BOOTHSOFT_SCRIPT=fichier.json` qui rejoue des
  touchers sans outil externe — préférable, fonctionne partout).
- Ajouter un chronométrage interne `BOOTHSOFT_TIMING=1` : toucher → première image,
  fin du décompte → photo affichée, lancement → premier live.
- Lancer le scénario complet (veille, live 5 min, photo, rafale, impression, maintenance),
  **Canon 2000D branché** : c'est la configuration de référence, la webcam simulée ne
  sert plus qu'aux tests automatiques.

**Acceptation.** Un tableau « PC cible » dans `docs/GUEST_UI.md` avec chaque ligne du § 1.

### V2 — Valider la version Windows (P0, M)

**Constat.** Trois parties n'ont jamais tourné sous Windows :
`src/autostart.rs` (registre), `printer::default_printer` (`GetDefaultPrinterW`),
et le rendu OpenGL via `glow`. Un PC sans pilote graphique n'expose qu'OpenGL 1.1 et
l'application ne démarre pas.

**À faire.**
- Compiler et tester sur le PC cible ; corriger ce qui casse.
- Si le pilote Intel pose problème : basculer eframe sur `wgpu` (DirectX 11/12), en
  gardant `live_gl.rs` derrière une interface commune, ou livrer ANGLE.
- Vérifier le lancement au démarrage avec ouverture de session automatique.

**Acceptation.** Démarrage à froid → live sans clavier ni souris, 3 fois de suite.

### B1 — Caméra débranchée, reprise automatique (P0, M) — détail technique de Z2

**Constat.** `src/camera.rs` : si la caméra disparaît (câble USB, batterie du 2000D,
extinction), l'écran « L'appareil photo fait
une pause » demande un toucher sur « Réessayer ». Rien ne retente seul.

**À faire.**
- Dans le fil caméra : après une erreur de lecture, retenter l'ouverture toutes les 2 s
  (puis 5 s), sans réveiller l'interface entre deux essais.
- Revenir au live automatiquement dès que le flux reprend.
- En maintenance : afficher la dernière erreur caméra et l'heure.

**Acceptation.** Débrancher puis rebrancher la caméra pendant le live : retour au live
en moins de 5 s sans toucher l'écran ; CPU ≤ 1 % pendant l'attente.

### B2 — Journal sur disque (P0, S)

> **Statut (11/11/2026) : fait.** `src/log.rs` : fil `bs-log`, file bornée (256 lignes,
> jamais bloquant), un fichier par jour dans `BoothSoft/logs/`, 14 jours gardés,
> paniques journalisées. Écrits : démarrage, thème, chaque changement d'écran et de
> statut, début de séance (événement, mode), ouverture de la maintenance, code PIN
> erroné, tout le gardien. Chemin affiché dans Données. **Reste à faire** : bouton
> « Exporter les journaux » (ZIP), statuts techniques en énumération (fiche L1).

**Constat.** Aucun journal : les erreurs vont dans `self.status` (perdu au prochain
message) ou sur la sortie d'erreur, invisible en mode borne. Les messages de statut
mélangent anglais et français.

**À faire.**
- Module `log.rs` maison (pas de dépendance) : une ligne horodatée par événement dans
  `BoothSoft/logs/AAAA-MM-JJ.log`, écriture par un fil dédié, 14 fichiers conservés.
- Journaliser : démarrage, événement actif, séances, impressions, erreurs caméra /
  imprimante / disque, entrées en maintenance.
- Page Données : « Exporter les journaux » (ZIP).
- Séparer message invité (français, rassurant) et message technique (journal).

**Acceptation.** Après une panne simulée, le journal permet de dire quoi et quand.
Coût : 0 % CPU au repos, < 1 Mo de RAM.

### B4 — Disque plein et nettoyage (P0, S)

> **Statut (15/11/2026) : fait.** `src/disk.rs` : espace libre lu à chaque début de
> séance (un appel système) et à l'ouverture de la maintenance. Sous 5 Go : alerte
> orange en Vue d'ensemble et Données ; sous 500 Mo : plus de nouvelle séance, message
> calme aux invités (« La borne fait une pause : sa mémoire est pleine. »). Données ›
> Événements enregistrés : taille de chaque événement, suppression avec confirmation
> (jamais l'événement en cours). **Reste** : suppression automatique après N jours.

**Constat.** Chaque photo est écrite deux fois (originale + version cadre). Rien ne
vérifie l'espace libre ni ne supprime les vieux événements. Une rafale de 10 pèse environ 5 à 8 Mo (estimé).

**À faire.**
- Vérifier l'espace libre au démarrage et toutes les 50 séances ; alerte en maintenance
  sous 5 Go, message invité et blocage propre sous 500 Mo.
- Page Données : liste des événements avec taille, « Exporter » puis « Supprimer »
  (suppression seulement après confirmation et code PIN).
- Option « supprimer automatiquement les événements de plus de N jours » (désactivée
  par défaut).

**Acceptation.** Disque plein simulé : aucune photo perdue ni fichier tronqué, message clair.

### A1 — Canon EOS 2000D en USB (P0, L)

**Constat.** L'appareil de la borne est un Canon EOS 2000D (Rebel T7 / 1500D).
Aujourd'hui `camera.rs` ne connaît que les webcams (`nokhwa`) : le 2000D n'est pas vu.

**Ce que l'appareil permet** (à confirmer appareil en main, voir « Points à vérifier ») :

| Élément | Valeur | Conséquence |
| --- | --- | --- |
| Visée en direct par USB | Images JPEG 1056×704 (rapport 3:2) | Passe tel quel dans `mjpeg.rs` ; 3:2 = format du tirage 6×4, donc plus de zones rognées à l'écran |
| Photo | Jusqu'à 6000×4000 (24 Mpx), tailles réduites M et S1 disponibles | Le tirage 1800×1200 n'a besoin que de la taille S1 (≈ 6 Mpx) |
| Pilotage | Kit Canon EDSDK (Windows, macOS, Linux) | Une seule intégration pour le PC de la borne et le Mac de développement |
| Alimentation | Batterie LP-E10 ; kit secteur Canon ACK-E10 | Kit secteur indispensable pour une soirée |

**Choix technique : Canon EDSDK**, pas de carte HDMI ni d'utilitaire webcam.
- EDSDK donne les trois besoins par le seul câble USB : visée, déclenchement, transfert
  de la photo directement vers le PC (aucune carte SD nécessaire).
- Sous Windows il utilise le pilote standard : rien à installer de plus sur la borne.
- Carte d'acquisition HDMI écartée : image vidéo seulement, pas la photo du capteur, et
  la sortie HDMI sans incrustations n'est pas confirmée sur ce modèle.
- EOS Webcam Utility écarté : même limite (vidéo seulement).
- libgphoto2 gardé en plan B pour Linux / macOS uniquement (sous Windows il oblige à
  remplacer le pilote USB).

**Prérequis côté exploitant.**
- Créer un compte sur le portail développeur Canon et télécharger EDSDK (gratuit, sur
  demande). Les fichiers du kit ne sont pas publics : ils se placent dans le **dépôt
  privé** de l'exploitant (`vendor/edsdk/`) et sont livrés à côté du programme sur ses
  bornes uniquement (organisation complète : `docs/EXTENSIONS.md`).
- Kit secteur ACK-E10, câble USB de l'appareil de bonne qualité (1,5 m max, fixé).

**À faire dans le code.**
0. **Lot A1.0 (dépôt public)** : point d'extension décrit dans `docs/EXTENSIONS.md` § 3 —
   application déplacée dans la bibliothèque (`boothsoft::app::run(Extensions)`),
   contrat `CameraProvider`, `FrameSource` enrichi (`capture`, `set_idle`, `keep_alive`,
   `status`). Aucune mention de Canon dans le dépôt public.
1. **Dépôt privé de l'exploitant** : crate `boothsoft-canon`, liaison minimale avec
   EDSDK (une vingtaine de fonctions), qui implémente `CameraProvider` / `FrameSource`.
   Le kit Canon n'est pas redistribuable : il reste dans le dépôt privé (`vendor/edsdk/`)
   et sur les bornes de l'exploitant. Tout ce qui suit et n'appelle pas le kit
   (points 2 à 10) se développe dans le dépôt public.
2. **Fil caméra unique** : EDSDK impose que tous les appels viennent du même fil ;
   c'est déjà l'architecture de `camera.rs` (fil `bs-camera`).
3. **Visée** : lire une image JPEG de visée à la cadence demandée (`live_fps`),
   décodage par `mjpeg.rs`, détection de mouvement et veille inchangées. En veille,
   couper la visée (miroir baissé, capteur au repos) : la présence ne peut plus être
   détectée par l'appareil, donc réveil au toucher uniquement, ou webcam d'appoint
   à 4 img/s si le réveil automatique est voulu.
4. **Photo** : déclenchement par EDSDK, enregistrement « vers l'ordinateur », réception
   du JPEG en mémoire puis écriture dans le dossier de l'événement.
   - Le fichier de l'appareil est gardé **tel quel** comme original (plus de
     ré-encodage : gain de CPU et qualité intacte).
   - La version « cadre » est produite en décodant à échelle réduite (décodage JPEG
     au ½ ou au ¼ intégré à libjpeg-turbo) : on n'a jamais 24 Mpx décompressés en mémoire.
5. **Réglage de la taille** : forcer la qualité JPEG « S1 » (≈ 2976×1984) par défaut au
   branchement, réglable en maintenance (S1 / M / L). Jamais de RAW.
6. **Mise au point** : la mise au point automatique en visée directe est lente sur ce
   modèle. Par défaut : mise au point faite une fois à l'installation (bouton
   « Faire la mise au point » en maintenance), puis déclenchements sans autofocus.
7. **Rafale** : chaque photo relève le miroir et transfère un fichier ; viser 1,5 s
   entre deux photos au lieu de 0,9 s (valeur à ajuster par mesure), et afficher la
   dernière image de visée figée pendant le déclenchement.
8. **Maintien en éveil** : envoyer la commande de prolongation toutes les minutes ;
   reprise automatique si l'appareil est éteint puis rallumé (rejoint la fiche B1).
9. **Maintenance** : page Caméra → liste « Canon EOS 2000D » en plus des webcams,
   niveau de batterie ou secteur, taille de photo, bouton de test avec aperçu.
10. **Repli** : si le Canon n'est pas détecté au démarrage, utiliser la webcam choisie
    et l'afficher clairement en maintenance.

**Réglages à faire une fois sur l'appareil** (à reprendre dans le guide de mise en service) :
molette sur M, extinction automatique désactivée, rotation automatique désactivée,
Wi-Fi désactivé, « déclencher sans carte » activé, exposition et balance des blancs
fixées pour le flash ou l'éclairage de la borne.

**Points à vérifier appareil en main** (avant d'écrire le code définitif) :
- cadence réelle de la visée par USB et son coût CPU sur l'i5 ;
- délai déclenchement → fichier reçu, en S1 et en L ;
- durée maximale de visée continue (échauffement du capteur) sur une soirée de 6 h ;
- comportement si la batterie factice est débranchée ;
- usure : une rafale de 10 = 10 déclenchements mécaniques ; prévoir un compteur
  de déclenchements par événement dans le bilan (fiche C5).

**Acceptation.**
- Brancher le 2000D : live à l'écran en moins de 5 s, sans réglage.
- Photo : original en taille S1 ou plus dans le dossier de l'événement ; fin du
  décompte → photo affichée ≤ 2,5 s.
- Live ≤ 15 % d'un cœur avec GPU ; RAM pic ≤ 260 Mo pendant une rafale de 5.
- 300 déclenchements d'affilée sans blocage ni fuite mémoire.
- Appareil éteint puis rallumé : retour au live sans toucher l'écran.

### A2 — Webcam : photo en pleine résolution (P3, M)

**Constat.** En mode webcam (secours ou démonstration), `camera.rs::read_capture` prend
une image du flux vidéo 1280×720, agrandie à 1800×1200 pour l'impression.

**À faire.** À l'ouverture, lister les formats de la webcam et retenir deux réglages :
live (720p, 30 img/s) et photo (résolution maximale). Flux unique en 1080p si le live
reste dans le budget, sinon bascule de format pendant la dernière seconde du décompte.

**Acceptation.** Fichier original ≥ 1920×1080 ; live toujours ≤ 15 % avec GPU.

### B3 — Suivi réel des impressions (P1, L)

**Constat.** `print_jobs.rs` : un tirage envoyé reste « à vérifier » jusqu'à une
confirmation manuelle en maintenance. Le journal `print-jobs.json` est réécrit en entier
à chaque tirage et ne se vide jamais (coût croissant avec le nombre de tirages).

**À faire.**
- Windows : interroger la file d'attente (`OpenPrinterW`, `GetJobW`, `EnumJobsW`) dans
  le fil d'impression, toutes les 2 s tant qu'un tirage est en cours, jamais au repos.
  États : en file, en impression, terminé, erreur (plus de papier, capot ouvert, hors ligne).
- macOS / Linux : `lpstat -W not-completed` selon le même rythme.
- Confirmation automatique quand le système déclare le tirage terminé ; l'écran
  « Impressions à vérifier » ne garde que les cas en erreur.
- Message invité si l'imprimante est en erreur avant d'accepter un tirage.
- Journal des tirages : un fichier par événement, ajout en fin de fichier (plus de
  réécriture complète), archivage à la fin de l'événement.

**Acceptation.** 50 tirages enchaînés sans action en maintenance ; papier retiré →
message invité en moins de 5 s ; coût d'un tirage constant quel que soit l'historique.

### P1 — Décodage du live sans conversion couleur CPU (P1, M)

**Constat.** `mjpeg.rs` décode chaque image du live en RGBA sur le CPU (≈ 6,5 % d'un
cœur à 30 img/s, mesuré). La conversion YCbCr → RGB et l'agrandissement de la
chrominance représentent une part importante de ce temps.

**À faire.**
- Décoder en plans bruts Y, Cb, Cr (`mozjpeg` : sortie brute, sans conversion).
- Envoyer trois textures (ou une texture atlas) et faire la conversion dans le shader
  de `live_gl.rs`, où la matrice du filtre est déjà appliquée : une seule matrice
  combinée, coût GPU nul.
- Images recyclées comme aujourd'hui ; volume envoyé au GPU divisé par 2,7
  (1,5 octet/pixel au lieu de 4).
- S'applique aussi à la visée du Canon 2000D (images JPEG 1056×704) : même chemin.
- Webcam de secours : si elle propose NV12 / YUY2 en 720p30, l'utiliser (plus de décodage JPEG).

**Acceptation.** Fil caméra ≤ 4,5 % à 30 img/s (estimé : −30 à −40 %) ; image identique
à l'œil ; RAM stable.

### P2 — Plus rien de lent sur le fil d'interface (P1, M)

**Constat.** Quatre opérations bloquent encore l'affichage :

| Où | Opération | Blocage estimé |
| --- | --- | --- |
| `burst.rs::ensure_big` | Décodage JPEG 1800×1200 à chaque changement de photo | 30–60 ms |
| `admin.rs::submit_pin` | Vérification bcrypt | 200–300 ms |
| `admin.rs` (logos, vignettes de thèmes), `kiosk.rs::ensure_art` | Décodage PNG | 10–80 ms |
| `kiosk.rs::resolve_printer` | Sous-processus `lpstat` au premier tirage | 20–100 ms |

**À faire.**
- Un fil « tâches lentes » unique avec file bornée (décodage, bcrypt, recherche
  d'imprimante), résultat renvoyé par canal + réveil de l'interface.
- Choix de rafale : afficher tout de suite la vignette agrandie, remplacer par la
  version nette à l'arrivée ; précharger la photo suivante.
- Code PIN : désactiver le clavier et afficher « Vérification… » pendant le calcul.
- Imprimante par défaut : la chercher au démarrage et après chaque passage en maintenance.

**Acceptation.** Aucune image d'interface au-dessus de 16 ms hors chargement de thème
(mesuré par `BOOTHSOFT_TIMING=1`).

### P3 — Index des séances par événement (P1, M)

**Constat.** `storage.rs::open` lit tous les `session.json` au démarrage et garde toutes
les séances en mémoire ; `start_session` les parcourt toutes pour la limite de séances.
Après une saison (des milliers de séances), le démarrage et la RAM grossissent.

**À faire.**
- Un fichier `event.json` par dossier d'événement : nombre de séances, de photos, de
  tirages, taille, dernière activité. Mis à jour à chaque séance.
- Ne charger en mémoire que l'événement en cours ; les autres sont lus à la demande
  par la page Données.
- `current_event` : mise en cache, recalcul une fois par minute et à chaque sauvegarde
  des réglages (aujourd'hui recalculé à chaque appel).

**Acceptation.** Avec 5 000 séances sur disque : démarrage identique à un disque vide
(± 100 ms), RAM identique (± 2 Mo).

### P4 — Petits gains mémoire et CPU (P2, S chacun)

- Libérer l'illustration d'accueil (jusqu'à 1920×1200, ~9 Mo) quand la borne est sur le
  live, la recharger à l'entrée en veille par le fil de tâches lentes.
- Vignettes de filtres : les dessiner dans une seule passe GPU (un quad instancié)
  au lieu de six appels.
- Mesurer la part de la police et de l'atlas egui dans les 140 Mo ; retirer les polices
  par défaut d'egui si elles ne servent plus (`default_fonts`).
- Sous Windows, comparer l'allocateur système et `mimalloc` sur le scénario complet.
- Rafale : réutiliser les tampons d'image entre deux photos (aujourd'hui une allocation
  de 3 à 6 Mo par photo).

### U1 — Parcours invité au niveau de dslrBooth (P2, M)

**Constat.** Le parcours est court, mais il manque des repères que les invités attendent.

**À faire.**
- **Sons** : bip du décompte, déclencheur, petit son à l'impression. Fichiers courts
  préchargés, volume réglable, désactivables. Lecture par un fil dédié, 0 % au repos.
- **Flash d'écran en éclairage** : écran blanc 150 ms *avant* la capture (sert de lumière
  d'appoint), en plus du flash actuel après.
- **Rafale** : bouton « Recommencer » sur l'écran de choix ; pause automatique si le
  développement prend du retard.
- **Réimpression** : en maintenance, « Réimprimer la dernière séance » ; côté invité,
  « Encore un tirage ? » sur l'écran de remerciement pendant 6 s.
- **Texte du décompte et durée** réglables (3, 5, 7 s).
- **Langue** : voir le chantier L1 (anglais par défaut, français, espagnol, chinois).
- **Curseur** : masqué en mode borne (aujourd'hui visible si une souris est branchée).

**Acceptation.** Test avec 5 personnes qui ne connaissent pas la borne : photo imprimée
sans aide en moins de 45 s.

### C2 — Format d'impression : 10×15 uniquement (décision prise)

Un seul format : **10×15 cm (4×6), une photo par tirage**, avec le cadre du thème.
Pas de bandes 2×6, pas de montage à plusieurs photos, pas de papier perforé.
Le mode rafale reste un mode de prise de vue (plusieurs photos, chacune imprimable
seule), pas un montage. Reste à faire sur ce format : composer à la taille exacte de
la zone imprimable de la DNP QW410 (voir « Spécificités DNP QW410 »).

### U2 — Gestes tactiles : suite (P2, M)

**Fait** (v0.16) : enfoncement des boutons, sélecteur « Une photo / Rafale » glissant,
arrivée des écrans et des fenêtres, photo de rafale qui suit le doigt avec butée
élastique, anneau de sélection glissant, chiffre du décompte qui « rebondit »,
interrupteurs et menu de la maintenance animés. Repos toujours à 0 % CPU.

**À faire.**
- Balayage horizontal sur le live pour changer de filtre (avec le nom du filtre qui passe).
- Inertie sur la bande de vignettes de la rafale et sur les pages de la maintenance.
- Onde sous le doigt à chaque toucher (retour visuel, utile sur un écran sans vibration).
- Fondu entre deux écrans au lieu d'un changement net (rendu de l'ancien écran dans
  une texture, 200 ms).
- Appui long, double appui et zoom à deux doigts volontairement ignorés : un seul geste
  par action, pour des invités qui découvrent la borne.
- Mesurer la latence toucher → image sur l'écran 15,6" (objectif ≤ 50 ms).

### M1 — Règles communes de la maintenance (P1, M)

> **Statut (11/11/2026) : lot M1.1 fait** (fenêtre « Quitter sans enregistrer ? » sur
> Retour aux photos, Fermer BoothSoft, Verrouiller et Annuler ; confirmation de
> suppression d'un événement). Lots M1.2 (historique) et M1.3 (brouillons) à faire.

Spécification complète : `docs/specs/MAINTENANCE_PATTERNS.md`. Vocabulaire thème /
événement / cadre ; fenêtre « Quitter sans enregistrer ? » (Enregistrer et quitter ·
Quitter sans enregistrer · Rester) sur toute sortie avec des modifications — elle
remplace l'enregistrement d'office actuel de « Retour aux photos » ; historique des
3 dernières versions des réglages, des thèmes et des cadres, restaurables ; brouillons
récupérables après verrouillage automatique ; suppressions toujours confirmées.

### F1 — Un cadre par événement (P1, XL en 5 lots)

Spécification complète : `docs/specs/EVENT_FRAMES.md`. Le cadre sort du thème : il
appartient à l'événement. Bibliothèque de cadres (livrés, importés en PNG, ou fabriqués
dans l'application : bordure, bandeau avec logo et deux lignes, polaroïd). Un même
modèle donne à chaque événement un tirage à son nom et à sa date. La borne applique le
bon cadre seule au changement de créneau. La photo remplit toujours tout le tirage sous
le cadre. Les thèmes « Mariage » et « Anniversaire » sont retirés, leurs cadres rejoignent
la bibliothèque. **Portrait** (lot F1.5) : chaque événement choisit paysage, portrait ou
choix de l'invité, et a un cadre pour chaque orientation ; l'image est tournée à
l'impression (sinon `printer.rs` la déformerait) ; réglage « caméra montée à la verticale ».

### K1 — Clavier tactile de la maintenance (P1, M)

Spécification complète : `docs/specs/TOUCH_KEYBOARD.md`. Sans lui, aucun champ de texte
de la maintenance n'est utilisable sur la borne (nom d'événement, textes du cadre).

### T1 — Éditeur du thème de l'entreprise (P2, L en 4 lots)

Spécification complète : `docs/specs/THEME_EDITOR.md`. Créer et modifier un thème
(couleurs avec contrôle de lisibilité, logo, fond d'accueil, textes), les thèmes livrés
par copie, aperçu sur les vrais écrans, dupliquer, exporter en ZIP, supprimer, revenir à
l'une des 3 versions précédentes. Le thème ne gère aucun cadre.

### G1 — Livre d'or vidéo (P2, XL en 5 lots) — ✅ fait (G1.6 MP4 facultatif)

Spécification complète : `docs/specs/GUEST_BOOK.md`. Fonction désactivée par défaut,
débloquée dans la maintenance. Troisième mode sur le live (« Une photo » reste
présélectionné) : test du micro avec jauge de volume en direct, puis message vidéo avec
le son, de 2 à 30 s (maximum réglable), relecture, garder ou recommencer. Aucun encodage
vidéo : les JPEG de la caméra sont écrits tels quels dans un AVI avec le son en PCM
(coût CPU quasi nul). Son par `cpal`, micro ouvert uniquement pendant ces écrans.
Messages rangés dans le dossier de l'événement, écoute et export dans la maintenance.

### L1 — Système de langues (P2, XL en 5 lots)

Spécification complète : `docs/specs/LANGUAGES.md`. Quatre langues : **anglais (par
défaut pour une installation neuve)**, français, espagnol, chinois simplifié. Les bornes
déjà en service restent en français à la mise à jour. Tous les textes en dur passent par
un module `i18n` vérifié à la compilation (impossible d'oublier une traduction), avec
pluriels et dates par langue. Langue réglée en maintenance, choix par l'invité en option.
Textes des thèmes par langue. Le chinois demande une police dédiée, chargée à la demande
pour ne rien coûter aux autres langues (budget : +12 Mo de RAM au plus quand il est actif).

### C3 — Réglages par événement (P2, M)

**Constat.** `model.rs::EventSlot` ne porte que le nom et les horaires (et bientôt le
cadre, fiche F1). Le nombre de tirages et le mode rafale sont globaux : il faut encore
passer en maintenance entre deux événements. Le thème, lui, reste celui de l'entreprise
et n'est **pas** réglable par événement.

**À faire.**
- Champs facultatifs par événement : tirages prévus, exemplaires max, rafale
  oui / non, limite de séances. Vide = réglage général.
- Bascule automatique au changement de créneau (à la minute près, jamais pendant une
  séance en cours).
- Bouton « Dupliquer » sur un événement ; import / export du planning (fichier JSON sur clé USB).

**Acceptation.** Deux événements le même jour avec deux cadres et deux nombres de
tirages : la borne change seule à l'heure prévue, sans redémarrage, sans toucher.

### C1 — GIF / boomerang depuis la rafale (P3, M)

- La rafale fournit déjà 3 à 10 images : assembler un GIF (ou MP4 court) dans le fil
  photo, à la fin de la séance, enregistré dans le dossier de l'événement.
- Aperçu sur l'écran de choix : défilement des vignettes à 4 img/s **uniquement** tant
  que l'aperçu est visible (seule exception tolérée à la règle « pas d'animation »).
- Sans partage en ligne, l'intérêt est limité à l'album remis au client : à faire après C2.

### C5 — Bilan d'événement et export (P3, S)

- Page Données : par événement, séances, photos, tirages, heure de pointe.
- « Exporter vers une clé USB » : copie du dossier de l'événement + planche-contact +
  bilan en texte. Copie faite par le fil de tâches lentes avec barre de progression.

### O1 — Santé du code (P3, M)

- `kiosk.rs` (≈ 1 450 lignes) : un fichier par écran (`welcome.rs`, `ready.rs`,
  `countdown.rs`, `review.rs`, `thanks.rs`), l'état commun reste dans `kiosk.rs`.
- Mode de test intégré (voir V1) : rejouer un scénario et comparer des captures d'écran
  de référence, pour détecter les régressions visuelles.
- Intégration continue : compilation et tests sur Windows, macOS et Linux à chaque envoi ;
  scénario de performance sur Linux avec seuils (échec si un budget du § 1 est dépassé).
- Statuts techniques : une énumération au lieu de chaînes en anglais comparées par préfixe
  (`admin.rs::render_top_bar` filtre aujourd'hui sur « Camera », « Photo »…).

### O3 — Publication open source (P1, S) — ✅ fait

Publié sur GitHub (`Ryvexam/BoothSoft`, CPAL-1.0 : crédit du créateur obligatoire à l'écran) avec un historique neuf. Le dépôt
ne contient aucune marque d'exploitant en dehors de la mention du créateur (`NOTICE`, crédits du README) : un seul thème livré (« Party », `default`),
identifiant de démarrage automatique `org.boothsoft.kiosk`, tests et exemples neutres
(« Léa & Hugo », `my-brand`). `LICENSE`, `NOTICE`, `README.md`, `CONTRIBUTING.md`,
`SECURITY.md`, `CODE_OF_CONDUCT.md`, `.gitignore` (`/private/`, `/vendor/`, thèmes
autres que `default`). Le kit Canon EDSDK n'est pas redistribuable : il vit dans un
dépôt privé qui dépend du dépôt public (`docs/EXTENSIONS.md`).

**Garde-fou.** Avant chaque publication : aucune marque d'exploitant ni nom de client
dans le dépôt (recherche automatique conseillée dans l'intégration continue).

**Acceptation.** Un inconnu clone le dépôt, compile, lance : il obtient une borne
fonctionnelle au thème Party.

### O2 — Installeur et borne verrouillée (P3, M)

- Installeur Windows (MSI) : programme, thèmes, raccourci, option « lancer au démarrage ».
- Signature du programme (évite l'alerte SmartScreen).
- Guide de mise en service d'un PC : ouverture de session automatique, veille et écran
  de veille désactivés, notifications et mises à jour reportées, gestes de bord d'écran
  et touche Windows neutralisés, compte sans droits d'administration.
- Instance unique : un second lancement ramène la fenêtre existante au premier plan.
- Chien de garde : si l'application s'arrête, elle est relancée (tâche planifiée Windows
  « au démarrage + si arrêt »), avec retour direct au live.

---

## 5. Protocole de mesure

1. Compiler en mode optimisé : `scripts/build-booth.sh`.
2. Scénario de référence : veille 30 s → live 60 s → photo simple → rafale de 5 →
   choix et impression de 3 tirages → retour live → maintenance 30 s.
3. Relever par étape : CPU moyen, RAM, pic de RAM, détail par fil (interface, caméra,
   photo, impression), nombre d'images dessinées par seconde.
4. Trois passes ; on retient la médiane.
5. Un chantier n'est terminé que si aucun budget du § 1 ne régresse.

---

## 6. Feuille de route proposée

| Étape | Contenu | Résultat |
| --- | --- | --- |
| 1 — Le Canon 2000D | A1, puis V1 et V2 avec l'appareil branché | La borne tourne avec son vrai matériel |
| 2 — Zéro intervention | Z1 à Z6 (elles englobent B1, B2, B3, B4) | Une soirée complète sans personne de chez l'exploitant |
| 3 — Fluidité | P1, P2, P3 | Zéro saccade, coût constant dans le temps |
| 4 — Personnalisation | M1, F1, K1, puis T1 et C3 (ordre détaillé dans `THEME_EDITOR.md` § 10) | Un cadre par événement, créé dans l'application et appliqué seul à l'heure prévue ; thème de l'entreprise modifiable |
| 5 — Produit de location | U1, U2, C2, O2, O3 (O3 au plus tard avant la première publication), L1 (langues) | Événements entièrement préparés, toucher fluide partout, installation simple |
| 6 — Extensions | C1, C5, A2, O1 | Parité de modes, webcam de secours en pleine résolution |

---

## 7. Hors périmètre (décisions prises)

- **Partage par QR code, e-mail, SMS, microsite** : écarté. Un QR code ne peut pas
  contenir une photo (≈ 3 Ko au maximum), et la borne fonctionne sans connexion.
  Si le sujet revient : point d'accès Wi-Fi local sur la borne (fiche à écrire), ou
  lien mis en ligne plus tard quand la connexion revient.
- **Fond vert, portraits par IA, lissage de peau, vidéo 360** : coût CPU / RAM
  incompatible avec le PC cible.
- **Paiement à l'impression, synchronisation cloud, tableau de bord web** : hors du
  modèle de location actuel.
- **Vidéo d'attraction en boucle** : contraire au budget « 0 % au repos » ; l'écran de
  veille reste une illustration fixe.
