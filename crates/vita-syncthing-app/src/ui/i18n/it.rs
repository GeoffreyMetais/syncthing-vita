//! Italian (`it`) string catalog for VitaSyncthing.

use super::{Messages, Strings};
use crate::domain::Language;

pub static STRINGS_IT: Strings = Strings {
    messages: &MESSAGES_IT,
    language: Language::Italian,

    // Common Actions
    action_cancel: "Annulla",
    action_save: "Salva",
    action_delete: "Elimina",
    action_done: "Fatto",
    action_close: "Chiudi",
    action_confirm: "Conferma",
    action_quit: "Esci",
    action_quit_app: "Esci dall'app",
    action_back: "Indietro",
    action_accept: "Accetta",
    action_decline: "Rifiuta",
    action_later: "Più tardi",
    action_up: "Su",
    action_open: "Apri",
    action_share: "Condividi",
    action_shared: "Condiviso",
    action_linked: "Collegato",
    action_not_linked: "Non collegato",
    action_pause: "Pausa",
    action_enable: "Abilita",
    action_devices: "Dispositivi",
    action_folder: "Cartella",
    action_folder_path: "Percorso cartella",
    action_add_by_id: "Aggiungi con ID",
    action_add_device: "Aggiungi dispositivo",
    action_add_device_id: "Aggiungi ID dispositivo",
    action_add_remote_device: "Aggiungi dispositivo remoto",
    action_scan_qr: "Scansiona QR",
    action_new_share: "Nuova condivisione",
    action_qr_id: "QR / ID",
    action_switch_theme: "Cambia tema",
    action_clear_declined: "Cancella rifiutati",
    action_virtual_keyboard: "Tastiera virtuale",
    action_hide_keyboard: "Nascondi tastiera",
    action_backspace: "Backspace",

    // Status Badges & Chips
    status_ready: "Pronto",
    status_syncing: "Sincronizzazione",
    status_scanning: "Scansione...",
    status_active: "Attivo",
    status_paused: "In pausa",
    share_mode_read_write: "Lettura e scrittura",
    share_mode_read_only: "Sola lettura",

    // Navigation & Sidebar
    tab_overview: "Panoramica",
    tab_files: "File",
    tab_settings: "Impostazioni",
    app_title: "VitaSync",
    app_version_label: "v0.1.0",
    sidebar_syncing: "Sincronizzazione...",
    sidebar_uploading: "Invio...",
    sidebar_last_sync: "Ultima sessione",
    protocol_tls: "BEP/1.0 TLS 1.3",
    protocol_discovery: "UDP 21027...",

    // Controller Hints
    hint_navigate: "Naviga",
    hint_select: "Seleziona",
    hint_toggle: "Attiva/Disattiva",
    hint_back: "Indietro",
    hint_tabs: "Schede",
    hint_quit: "Esci",
    hint_move: "Sposta",
    hint_folder: "Cartella",
    hint_folder_path: "Percorso cartella",
    hint_share_folder: "Condividi cartella",
    hint_select_target: "Seleziona destinazione",
    hint_open_subfolder: "Apri sottocartella",
    hint_up_one_level: "Livello superiore",
    hint_up_back: "Su / Indietro",
    hint_back_to_device_id: "Torna all'ID dispositivo",
    hint_pause_enable: "Pausa/Abilita",
    hint_add_by_id: "Aggiungi con ID",
    hint_scan_qr: "Scansiona QR",
    hint_scan_lan: "Cerca in rete",
    hint_confirm_delete: "Conferma eliminazione",
    hint_confirm_quit: "Conferma uscita",
    hint_create_pick_devices: "Crea e scegli disp.",
    hint_add_remote_device_id: "Aggiungi ID remoto",
    hint_close_qr_device_id: "Chiudi QR / ID",
    hint_accept_share: "Accetta condivisione",
    hint_confirm_add: "Conferma e aggiungi",
    hint_type_key: "Premi tasto",

    // Screen 1: Dashboard (Overview)
    dashboard_title: "Panoramica",
    hero_this_device: "Questo dispositivo",
    hero_device_name: "PlayStation Vita",
    hero_device_id_label: "ID dispositivo",

    // Screen 2: Shared Folders
    msg_no_shares: "Nessuna cartella condivisa configurata",
    msg_no_shares_hint_1:
        "Premi Triangolo (Nuova condivisione) per creare una cartella su PS Vita,",
    msg_no_shares_hint_2: "o tocca la scheda di sincronizzazione per importare cartelle condivise.",

    // Screen 3: Files & Folder Picker
    picker_title_create: "Condividi una cartella",
    picker_subtitle_create: "Seleziona una cartella da condividere, aprila per esplorare i file",
    picker_title_choose: "Scegli la cartella di destinazione",
    picker_default_share: "Condivisione predefinita",

    // Screen 4: Settings & Protocol Log
    settings_title: "Impostazioni e registro",
    settings_subtitle: "Schermo e BEP/1.0",
    settings_sec_display: "SCHERMO E SALVASCHERMO OLED",
    settings_label_colour_theme: "Tema colori",
    settings_desc_theme_dark: "Scuro — Design blu ardesia VitaSend con accento turchese",
    settings_desc_theme_oled: "OLED — Fondo nero puro (risparmia batteria OLED)",
    settings_saver_auto_hint:
        "Il salvaschermo OLED si attiva dopo 12s di sincronizzazione (45s a riposo).",
    settings_sec_devices: "DISPOSITIVI E ABBINAMENTO",
    settings_desc_devices: "Aggiungi un dispositivo o vedi l'ID di questa PS Vita",
    settings_sec_log: "REGISTRO PROTOCOLLO BEP/1.0 E TLS 1.3",
    settings_sec_declined: "CARTELLE RIFIUTATE",
    settings_no_declined: "Nessuna · le cartelle offerte dai peer vengono sempre richieste prima",
    settings_sec_language: "LINGUA",
    settings_language_system: "Lingua di sistema",
    action_change_language: "Cambia lingua",

    // Screensaver
    saver_wake_hint: "Tocca lo schermo o premi un tasto per riattivare",

    // Modals: Create Share
    modal_create_share_title: "Crea nuova condivisione",
    modal_create_share_subtitle: "Scegli una cartella predefinita o esplora ux0: per condividere",
    modal_create_share_browse_sub: "Apri l'esploratore di directory interattivo ux0:",

    // Modals: Manage Devices
    modal_manage_devices_default_title: "Condivisione",
    modal_manage_devices_subtitle_incomplete:
        "Collega almeno un dispositivo per completare la condivisione",
    modal_manage_devices_subtitle:
        "Seleziona i dispositivi Syncthing che devono sincronizzare questa cartella",
    modal_manage_devices_scanning:
        "Ricerca peer Syncthing sulla rete locale (UDP 21027 + TCP 22000)...",
    modal_manage_devices_empty: "Nessun dispositivo Syncthing remoto ancora rilevato.",
    modal_manage_devices_empty_hint:
        "Premi Quadrato (Aggiungi con ID) o Triangolo per scansionare.",

    // Modals: Add Device
    modal_add_device_title: "Aggiungi dispositivo remoto tramite ID",
    modal_add_device_subtitle: "Inserisci l'ID breve a 7 caratteri o l'ID completo a 56 caratteri",
    modal_add_device_example: "es. MFZWI3D o MFZWI3D-BONSGYC-... (tocca per digitare)",
    modal_add_device_tip: "Suggerimento: bastano i primi 7 caratteri (ID breve in Syncthing).",
    modal_add_device_placeholder: "Inserisci ID breve a 7 car. o completo a 56 car...",
    modal_add_device_name_label: "Nome del dispositivo (facoltativo):",
    modal_add_device_name_empty: "(tocca per inserire il nome)",
    modal_add_device_confirm: "Aggiungi dispositivo",

    // Modals: Confirm Delete
    modal_delete_share_title: "Eliminare questa condivisione?",
    modal_delete_share_subtitle:
        "La cartella non sarà più sincronizzata con i dispositivi collegati",
    modal_delete_share_decline_hint:
        "Le future offerte dei peer verranno rifiutate (modificabile in Impostazioni).",

    // Modals: Confirm Revert (read-only share)
    action_revert: "Ripristina",
    modal_revert_title: "Annullare le modifiche locali?",
    modal_revert_subtitle: "La versione dei dispositivi sostituisce quella della PS Vita",
    modal_revert_added_hint: "I file aggiunti sulla PS Vita vengono eliminati.",
    modal_revert_restored_hint: "I file modificati o eliminati vengono riscaricati.",

    // Modals: Confirm Quit
    modal_quit_title: "Uscire da VitaSync?",
    modal_quit_subtitle: "Conferma per chiudere l'applicazione e tornare alla LiveArea",
    modal_quit_syncing_warn: "Una sincronizzazione BEP/1.0 è attualmente in corso.",
    modal_quit_syncing_hint: "Uscire adesso interromperà i trasferimenti di file attivi.",
    modal_quit_idle_ready: "Sei sicuro di voler uscire da VitaSync?",
    modal_quit_idle_hint: "Tutte le configurazioni e impostazioni sono salvate su ux0:.",

    // Modals: Device ID
    modal_device_id_title: "ID dispositivo PS Vita Syncthing",
    modal_device_id_subtitle:
        "Scansiona questo codice QR in Syncthing o inserisci l'ID a 56 caratteri",
    modal_device_id_scan_web: "Scansiona in Syncthing (Web / App)",
    modal_device_id_dev_name: "Nome dispositivo: PlayStation Vita (vita-syncthing)",
    modal_device_id_cert_path: "Certificato identità: ux0:data/syncthing/cert.der",
    modal_device_id_short_label: "ID breve (rilevamento LAN):",
    modal_device_id_full_label: "ID dispositivo completo a 56 caratteri:",
    modal_device_id_scan_hint:
        "Su smartphone/PC: Syncthing > Dispositivi > Aggiungi > Scansiona QR",

    // Modals: Folder Offer
    modal_folder_offer_title: "Nuova cartella condivisa",
    modal_folder_offer_hint:
        "Accetta per scegliere una cartella su ux0:. Nessun sync prima dell'attivazione.",

    // Modals: Scan QR
    modal_scan_qr_title: "Scansiona codice QR del dispositivo",
    modal_scan_qr_subtitle:
        "Syncthing: Azioni > Mostra ID, poi inquadra con la fotocamera posteriore",
};

/// IT templates for `UiText` messages.
pub static MESSAGES_IT: Messages = Messages {
    ready_tap_sync_card: "Pronto · Tocca la scheda di sincronizzazione",
    ready_manual_pairing: "Pronto (LAN manuale)",
    no_peer_linked: "Nessun peer collegato",
    select_device_to_connect: "Collega un dispositivo nei Dispositivi di una condivisione",
    no_folders_synced: "Nessuna cartella sincronizzata",
    no_remote_folders: "Nessuna cartella remota condivisa",
    starting_sync: "Avvio della sincronizzazione...",
    scanning_lan: "Ricerca di peer Syncthing nella LAN (UDP 21027)...",
    connecting_to: "Connessione TLS 1.3 a {name} ({addr})...",
    exchanging_index: "Scambio di ClusterConfig e indice BEP...",
    file_up_to_date: "Aggiornato: {file}",
    downloading: "Download di {file} ({pct}%)",
    index_published: "Indice locale della Vita pubblicato ({files} file)",
    uploading: "Invio al peer: {file}",
    sync_finished: "Terminato",
    connections_one: "Connesso (1 connessione)",
    connections_many: "Connesso (1+{secondary} connessioni)",
    sync_summary: "{connections} · {up_to_date} aggiornati | {downloaded} ricevuti ({kb} KB) | {uploaded} inviati ({bytes} B)",
    found_lan_peers: "{count} peer trovato/i nella LAN. Scegli un dispositivo nei Dispositivi di una condivisione.",
    no_peer_add_device: "Nessun peer collegato. Aggiungi un dispositivo nei Dispositivi di una condivisione o Impostazioni.",
    discovery_error: "Errore di rilevamento LAN: {error}",
    syncing_with_peers: "Sincronizzazione con i peer della LAN...",
    theme_dark: "Tema: Scuro (grigio-blu)",
    theme_oled: "Tema: OLED (nero puro #000000)",
    language_changed: "Lingua: {name}",
    device_linked: "{device} aggiunto a '{share}'",
    device_unlinked: "{device} rimosso da '{share}'",
    added_short_id: "ID breve {prefix} aggiunto (abbinamento automatico nella LAN)",
    added_device: "Dispositivo {name} aggiunto ({short})",
    own_device_id: "Impossibile aggiungere l'ID di questa PS Vita",
    qr_not_device_id: "Questo codice QR non è un ID dispositivo Syncthing",
    share_deleted: "Condivisione '{label}' eliminata",
    link_device_to_share: "Collega un dispositivo per condividere '{label}'",
    share_created: "Condivisione '{label}' creata",
    share_not_created: "Condivisione '{label}' non creata: nessun dispositivo collegato",
    saved_in: "Salvato in {path}",
    share_active: "La condivisione {id} è ora attiva",
    share_paused: "La condivisione {id} è ora in pausa",
    share_read_only: "La condivisione {id} è ora in sola lettura",
    share_read_write: "La condivisione {id} è ora in lettura e scrittura",
    revert_requested: "Annullamento delle modifiche locali di {id}...",
    local_changes_reverted: "Modifiche locali di {id} annullate ({count})",
    offer_accepted: "Condivisione '{label}' accettata ({id})",
    choose_where_to_store: "Scegli dove salvare '{label}'",
    offer_declined: "Condivisione '{label}' rifiutata ({id})",
    no_declined_folders: "Nessuna cartella rifiutata",
    declined_cleared: "{count} cartella/e rifiutata/e cancellata/e: verranno riproposte",
    id_empty: "L'ID dispositivo non può essere vuoto",
    id_invalid_char: "Carattere non valido '{ch}' (Base32: A-Z, 2-7)",
    id_too_short: "Inserisci almeno 7 caratteri (ID breve) o l'ID completo di 56",
    id_too_long: "ID dispositivo troppo lungo ({len} caratteri, max 56)",
    id_checksum: "Errore nei gruppi {first}-{second} (cifra di controllo)",
    id_decode_failed: "Impossibile decodificare l'ID dispositivo",
    no_devices_linked: "Nessun dispositivo collegato (Croce per aggiungerne uno)",
    shared_with: "Condivisa con: {devices}",
    picker_share_this_folder: "Condividi questa cartella ({path})",
    picker_keep_current: "Mantieni la cartella attuale ({path})",
    picker_save_data: "Salvataggi ({path})",
    picker_pictures: "Immagini PS Vita ({path})",
    picker_music: "Musica PS Vita ({path})",
    picker_retroarch: "Salvataggi RetroArch ({path})",
    picker_default_folder: "Cartella Syncthing predefinita ({path})",
    picker_browsed_folder: "Usa la cartella sfogliata ({path})",
    device_id_section: "ID DISPOSITIVO SYNCTHING (8 GRUPPI DA 7)",
    device_id_groups_1_4: "GRUPPI 1 - 4",
    device_id_groups_5_8: "GRUPPI 5 - 8",
    short_id_label: "ID breve: {id}",
    camera_starting: "Avvio della fotocamera posteriore...",
    camera_error: "{error} · Inserisci l'ID manualmente",
    add_device_matches_peer: "Corrisponde al peer LAN {name} ({short} @ {addr}) · Pronto da collegare!",
    add_device_valid_short: "ID breve valido ({len} car.: {short}) · Abbinamento automatico nella LAN",
    add_device_valid_full: "ID dispositivo Syncthing completo valido ({short})",
    device_row_short: "ID breve: {prefix} (risolto nella LAN)  ·  Indirizzo: {addr}",
    device_row_full: "ID: {id}  ·  Indirizzo: {addr}",
    unknown_device: "Dispositivo {short}",
};
