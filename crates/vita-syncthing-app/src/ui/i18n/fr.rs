//! French (`fr`) string catalog for VitaSyncthing.

use super::{Messages, Strings};
use crate::domain::Language;

pub static STRINGS_FR: Strings = Strings {
    messages: &MESSAGES_FR,
    language: Language::French,

    // Common Actions
    action_cancel: "Annuler",
    action_save: "Enregistrer",
    action_delete: "Supprimer",
    action_done: "Terminé",
    action_close: "Fermer",
    action_confirm: "Confirmer",
    action_quit: "Quitter",
    action_quit_app: "Quitter l'application",
    action_back: "Retour",
    action_accept: "Accepter",
    action_decline: "Refuser",
    action_later: "Plus tard",
    action_up: "Haut",
    action_open: "Ouvrir",
    action_share: "Partager",
    action_shared: "Partagé",
    action_linked: "Lié",
    action_not_linked: "Non lié",
    action_pause: "Suspendre",
    action_enable: "Activer",
    action_devices: "Appareils",
    action_folder: "Dossier",
    action_folder_path: "Chemin du dossier",
    action_add_by_id: "Ajouter par ID",
    action_add_device: "Ajouter appareil",
    action_add_device_id: "Ajouter ID d'appareil",
    action_add_remote_device: "Ajouter appareil distant",
    action_scan_qr: "Scanner QR",
    action_new_share: "Nouveau partage",
    action_qr_id: "QR / ID",
    action_switch_theme: "Changer thème",
    action_clear_declined: "Effacer refusés",
    action_virtual_keyboard: "Clavier virtuel",
    action_hide_keyboard: "Masquer clavier",
    action_backspace: "Effacer",

    // Status Badges & Chips
    status_ready: "Prêt",
    status_syncing: "Synchronisation",
    status_scanning: "Analyse...",
    status_active: "Actif",
    status_paused: "En pause",
    share_mode_read_write: "Lecture & écriture",
    share_mode_read_only: "Lecture seule",

    // Navigation & Sidebar
    tab_overview: "Aperçu",
    tab_files: "Fichiers",
    tab_settings: "Paramètres",
    app_title: "VitaSync",
    app_version_label: "v0.1.0",
    sidebar_syncing: "Synchronisation...",
    sidebar_uploading: "Envoi...",
    sidebar_last_sync: "Dernière session",
    protocol_tls: "BEP/1.0 TLS 1.3",
    protocol_discovery: "UDP 21027...",

    // Controller Hints
    hint_navigate: "Naviguer",
    hint_select: "Sélectionner",
    hint_toggle: "Basculer",
    hint_back: "Retour",
    hint_tabs: "Onglets",
    hint_quit: "Quitter",
    hint_move: "Déplacer",
    hint_screensaver: "Écran de veille",
    hint_folder: "Dossier",
    hint_folder_path: "Chemin du dossier",
    hint_share_folder: "Partager dossier",
    hint_select_target: "Choisir cible",
    hint_open_subfolder: "Ouvrir sous-dossier",
    hint_up_one_level: "Dossier parent",
    hint_up_back: "Haut / Retour",
    hint_back_to_device_id: "Retour à l'ID",
    hint_pause_enable: "Pause/Activer",
    hint_add_by_id: "Ajouter par ID",
    hint_scan_qr: "Scanner QR",
    hint_scan_lan: "Scanner le réseau",
    hint_confirm_delete: "Confirmer suppression",
    hint_confirm_quit: "Confirmer quitter",
    hint_create_pick_devices: "Créer & Choisir appareils",
    hint_add_remote_device_id: "Ajouter ID distant",
    hint_close_qr_device_id: "Fermer QR / ID",
    hint_accept_share: "Accepter partage",
    hint_confirm_add: "Confirmer & Ajouter",
    hint_type_key: "Saisir touche",

    // Screen 1: Dashboard (Overview)
    dashboard_title: "Aperçu",
    hero_this_device: "Cet appareil",
    hero_device_name: "PlayStation Vita",
    hero_device_id_label: "ID de l'appareil",

    // Screen 2: Shared Folders
    msg_no_shares: "Aucun dossier partagé configuré",
    msg_no_shares_hint_1:
        "Appuyez sur Triangle (Nouveau partage) pour créer un partage sur votre PS Vita,",
    msg_no_shares_hint_2: "ou touchez la carte de synchro pour importer les dossiers d'un pair.",

    // Screen 3: Files & Folder Picker
    picker_title_create: "Partager un dossier",
    picker_subtitle_create:
        "Sélectionnez un dossier à partager, ouvrez-le pour explorer ses sous-dossiers",
    picker_title_choose: "Choisir le dossier cible",
    picker_default_share: "Partage par défaut",

    // Screen 4: Settings & Protocol Log
    settings_title: "Paramètres & Journal",
    settings_subtitle: "Affichage & BEP/1.0",
    settings_sec_display: "AFFICHAGE ET ÉCONOMISEUR OLED",
    settings_label_colour_theme: "Thème de couleur",
    settings_desc_theme_dark: "Sombre — Aspect bleu ardoise VitaSend avec accent turquoise",
    settings_desc_theme_oled: "OLED — Fond noir pur (économise la batterie PS Vita OLED)",
    settings_saver_auto_hint:
        "L'économiseur d'écran OLED s'active après 12s en sync (ou touche SELECT).",
    settings_sec_devices: "APPAREILS & APPAIRAGE",
    settings_desc_devices: "Associer un appareil ou afficher l'ID de la PS Vita",
    settings_sec_log: "JOURNAL PROTOCOLE BEP/1.0 & TLS 1.3",
    settings_sec_declined: "DOSSIERS REFUSÉS",
    settings_no_declined: "Aucun · les dossiers proposés par les pairs sont toujours demandés",
    settings_sec_language: "LANGUE",
    settings_language_system: "Langue du système",
    action_change_language: "Changer de langue",

    // Log viewer
    logs_title: "Journal du protocole",
    logs_scope_session: "Cette session",
    logs_scope_history: "Historique complet · syncthing.log",
    logs_empty: "Aucune entrée pour l'instant",
    action_show_all: "Tout afficher",
    action_show_session: "Cette session",
    hint_scroll: "Défiler",
    hint_older: "Plus anciens",
    hint_newer: "Plus récents",

    // Screensaver
    saver_wake_hint: "Touchez l'écran ou appuyez sur une touche pour réactiver",

    // Modals: Create Share
    modal_create_share_title: "Créer un nouveau partage",
    modal_create_share_subtitle:
        "Choisissez un dossier prédéfini ou explorez ux0: pour créer un partage",
    modal_create_share_browse_sub: "Ouvrir l'explorateur interactif de répertoires ux0:",

    // Modals: Manage Devices
    modal_manage_devices_default_title: "Partage",
    modal_manage_devices_subtitle_incomplete:
        "Liez au moins un appareil pour finaliser la création de ce partage",
    modal_manage_devices_subtitle:
        "Cochez les appareils Syncthing distants qui doivent synchroniser ce dossier",
    modal_manage_devices_scanning:
        "Recherche de pairs Syncthing sur le réseau local (UDP 21027 + TCP 22000)...",
    modal_manage_devices_empty: "Aucun appareil Syncthing distant découvert pour le moment.",
    modal_manage_devices_empty_hint:
        "Appuyez sur Carré (Ajouter par ID) ou Triangle pour scanner le réseau.",

    // Modals: Add Device
    modal_add_device_title: "Ajouter un appareil distant par ID",
    modal_add_device_subtitle:
        "Saisissez l'ID court à 7 caractères (recommandé) ou l'ID complet à 56 caractères",
    modal_add_device_example: "ex. MFZWI3D  ou  MFZWI3D-BONSGYC-... (toucher pour saisir)",
    modal_add_device_tip: "Astuce : seuls les 7 premiers caractères (ID court) sont nécessaires.",
    modal_add_device_placeholder: "Saisir ID court 7 car. ou complet 56 car...",
    modal_add_device_name_label: "Nom de l'appareil (facultatif) :",
    modal_add_device_name_empty: "(toucher pour saisir un nom)",
    modal_add_device_confirm: "Ajouter l'appareil",

    // Modals: Confirm Delete
    modal_delete_share_title: "Supprimer ce partage ?",
    modal_delete_share_subtitle: "Le dossier cesse de se synchroniser avec tous les appareils liés",
    modal_delete_share_decline_hint:
        "Les offres futures des pairs seront refusées (modifiable dans Paramètres).",

    // Modals: Confirm Revert (read-only share)
    action_revert: "Restaurer",
    modal_revert_title: "Annuler les modifications locales ?",
    modal_revert_subtitle: "La version des pairs remplace celle de la PS Vita",
    modal_revert_added_hint: "Les fichiers ajoutés sur la PS Vita sont supprimés.",
    modal_revert_restored_hint: "Les fichiers modifiés ou supprimés sont retéléchargés.",

    // Modals: Confirm Quit
    modal_quit_title: "Quitter VitaSync ?",
    modal_quit_subtitle: "Confirmez si vous souhaitez quitter et retourner à la LiveArea",
    modal_quit_syncing_warn: "Une synchronisation BEP/1.0 est actuellement en cours.",
    modal_quit_syncing_hint: "Quitter maintenant interrompra les transferts de fichiers actifs.",
    modal_quit_idle_ready: "Toutes les synchronisations sont actuellement inactives.",
    modal_quit_idle_hint:
        "Toutes les configurations de partage et paramètres sont enregistrés sur ux0:.",

    // Modals: Device ID
    modal_device_id_title: "ID de l'appareil PS Vita Syncthing",
    modal_device_id_subtitle:
        "Scannez ce QR code dans Syncthing (Ajouter un appareil) ou saisissez l'ID",
    modal_device_id_scan_web: "Scannez dans Syncthing (Web / Mobile)",
    modal_device_id_dev_name: "Nom de l'appareil : PlayStation Vita (vita-syncthing)",
    modal_device_id_cert_path: "Certificat d'identité : ux0:data/syncthing/cert.der",
    modal_device_id_short_label: "ID court (découverte LAN) :",
    modal_device_id_full_label: "ID de l'appareil à 56 caractères :",
    modal_device_id_scan_hint: "Sur tél./PC : Syncthing > Appareils > Ajouter > Scanner QR",

    // Modals: Folder Offer
    modal_folder_offer_title: "Nouveau dossier partagé",
    modal_folder_offer_hint:
        "Acceptez pour choisir un dossier sur ux0:. Rien ne se synchronise avant activation.",

    // Modals: Scan QR
    modal_scan_qr_title: "Scanner le QR code de l'appareil",
    modal_scan_qr_subtitle:
        "Syncthing : Actions > Afficher l'ID, puis visez avec la caméra arrière",
};

/// FR templates for `UiText` messages.
pub static MESSAGES_FR: Messages = Messages {
    ready_tap_sync_card: "Prêt · Touchez la carte de synchro pour synchroniser",
    ready_manual_pairing: "Prêt (appairage LAN manuel)",
    no_peer_linked: "Aucun pair lié",
    select_device_to_connect: "Liez un appareil dans les Appareils d'un partage",
    no_folders_synced: "Aucun dossier synchronisé",
    no_remote_folders: "Aucun dossier distant partagé",
    starting_sync: "Démarrage de la synchronisation...",
    scanning_lan: "Recherche de pairs Syncthing sur le LAN (UDP 21027)...",
    connecting_to: "Connexion TLS 1.3 à {name} ({addr})...",
    exchanging_index: "Échange du ClusterConfig et de l'index BEP...",
    file_up_to_date: "À jour : {file}",
    downloading: "Téléchargement de {file} ({pct} %)",
    index_published: "Index local de la Vita publié ({files} fichiers)",
    uploading: "Envoi au pair : {file}",
    sync_finished: "Terminé",
    connections_one: "Connecté (1 connexion)",
    connections_many: "Connecté (1+{secondary} connexions)",
    sync_summary: "{connections} · {up_to_date} à jour | {downloaded} reçus ({kb} Ko) | {uploaded} envoyés ({bytes} o)",
    found_lan_peers: "{count} pair(s) trouvé(s) sur le LAN. Choisissez un appareil dans les Appareils d'un partage.",
    no_peer_add_device: "Aucun pair lié. Ajoutez un appareil dans les Appareils d'un partage ou Paramètres.",
    discovery_error: "Erreur de découverte LAN : {error}",
    syncing_with_peers: "Synchronisation avec les pairs du LAN...",
    theme_dark: "Thème : Sombre (bleu-gris)",
    theme_oled: "Thème : OLED (noir pur #000000)",
    language_changed: "Langue : {name}",
    device_linked: "{device} ajouté à « {share} »",
    device_unlinked: "{device} retiré de « {share} »",
    added_short_id: "ID court {prefix} ajouté (appairage auto sur le LAN)",
    added_device: "Appareil {name} ajouté ({short})",
    own_device_id: "Impossible d'ajouter l'ID de cette PS Vita",
    qr_not_device_id: "Ce QR code n'est pas un ID d'appareil Syncthing",
    share_deleted: "Partage « {label} » supprimé",
    link_device_to_share: "Liez un appareil pour partager « {label} »",
    share_created: "Partage « {label} » créé",
    share_not_created: "Partage « {label} » non créé : aucun appareil lié",
    saved_in: "Enregistré dans {path}",
    share_active: "Le partage {id} est actif",
    share_paused: "Le partage {id} est en pause",
    share_read_only: "Le partage {id} est en lecture seule",
    share_read_write: "Le partage {id} est en lecture & écriture",
    revert_requested: "Annulation des modifications locales de {id}...",
    local_changes_reverted: "Modifications locales de {id} annulées ({count})",
    offer_accepted: "Partage « {label} » accepté ({id})",
    choose_where_to_store: "Choisissez où enregistrer « {label} »",
    offer_declined: "Partage « {label} » refusé ({id})",
    no_declined_folders: "Aucun dossier refusé",
    declined_cleared: "{count} dossier(s) refusé(s) effacé(s) : ils seront proposés à nouveau",
    id_empty: "L'ID d'appareil ne peut pas être vide",
    id_invalid_char: "Caractère invalide « {ch} » (Base32 : A-Z, 2-7)",
    id_too_short: "Saisissez au moins 7 caractères (ID court) ou l'ID complet de 56 caractères",
    id_too_long: "ID d'appareil trop long ({len} caractères, 56 max.)",
    id_checksum: "Faute de frappe dans les groupes {first}-{second} (somme de contrôle)",
    id_decode_failed: "Impossible de décoder l'ID d'appareil",
    no_devices_linked: "Aucun appareil lié (Croix pour ajouter un appareil)",
    shared_with: "Partagé avec : {devices}",
    picker_share_this_folder: "Partager ce dossier ({path})",
    picker_keep_current: "Garder le dossier actuel ({path})",
    picker_save_data: "Sauvegardes ({path})",
    picker_pictures: "Images PS Vita ({path})",
    picker_music: "Musique PS Vita ({path})",
    picker_retroarch: "Sauvegardes RetroArch ({path})",
    picker_default_folder: "Dossier Syncthing par défaut ({path})",
    picker_browsed_folder: "Utiliser le dossier parcouru ({path})",
    device_id_section: "ID D'APPAREIL SYNCTHING (8 GROUPES DE 7)",
    device_id_groups_1_4: "GROUPES 1 - 4",
    device_id_groups_5_8: "GROUPES 5 - 8",
    short_id_label: "ID court : {id}",
    camera_starting: "Démarrage de la caméra arrière...",
    camera_error: "{error} · Saisissez plutôt l'ID à la main",
    add_device_matches_peer: "Correspond au pair LAN {name} ({short} @ {addr}) · Prêt à lier !",
    add_device_valid_short: "ID court valide ({len} car. : {short}) · Appairage auto avec le pair LAN",
    add_device_valid_full: "ID d'appareil Syncthing complet valide ({short})",
    device_row_short: "ID court : {prefix} (résolu sur le LAN)  ·  Adresse : {addr}",
    device_row_full: "ID : {id}  ·  Adresse : {addr}",
    unknown_device: "Appareil {short}",
};
