//! Spanish (`es`) string catalog for VitaSyncthing.

use crate::domain::Language;
use super::Strings;

pub static STRINGS_ES: Strings = Strings {
    language: Language::Spanish,

    // Common Actions
    action_cancel: "Cancelar",
    action_save: "Guardar",
    action_delete: "Eliminar",
    action_done: "Listo",
    action_close: "Cerrar",
    action_confirm: "Confirmar",
    action_quit: "Salir",
    action_quit_app: "Salir de la aplicación",
    action_back: "Atrás",
    action_accept: "Aceptar",
    action_decline: "Rechazar",
    action_later: "Más tarde",
    action_up: "Subir",
    action_open: "Abrir",
    action_share: "Compartir",
    action_shared: "Compartido",
    action_linked: "Vinculado",
    action_not_linked: "No vinculado",
    action_pause: "Pausar",
    action_enable: "Activar",
    action_devices: "Dispositivos",
    action_folder: "Carpeta",
    action_folder_path: "Ruta de carpeta",
    action_add_by_id: "Añadir por ID",
    action_add_device: "Añadir dispositivo",
    action_add_device_id: "Añadir ID dispositivo",
    action_add_remote_device: "Añadir dispositivo remoto",
    action_apply_sync: "Aplicar y sinc",
    action_scan_qr: "Escanear QR",
    action_new_share: "Nueva carpeta",
    action_sync_now: "Sincronizar",
    action_qr_id: "QR / ID",
    action_switch_theme: "Cambiar tema",
    action_clear_declined: "Borrar rechazados",
    action_virtual_keyboard: "Teclado virtual",
    action_hide_keyboard: "Ocultar teclado",
    action_backspace: "Retroceso",

    // Status Badges & Chips
    status_ready: "Listo",
    status_syncing: "Sincronizando",
    status_scanning: "Escaneando...",
    status_active: "Activo",
    status_paused: "En pausa",

    // Navigation & Sidebar
    tab_overview: "Resumen",
    tab_shares: "Carpetas",
    tab_files: "Archivos",
    tab_settings: "Ajustes",
    app_title: "VitaSync",
    app_version_label: "v0.1.0 · Pure Rust",
    sidebar_syncing: "Sincronizando BEP/1.0...",
    sidebar_last_sync: "Última sesión",
    protocol_tls: "BEP/1.0 TLS 1.3",
    protocol_discovery: "UDP 21027...",

    // Controller Hints
    hint_navigate: "Navegar",
    hint_select: "Seleccionar",
    hint_toggle: "Alternar",
    hint_back: "Atrás",
    hint_tabs: "Pestañas",
    hint_quit: "Salir",
    hint_move: "Mover",
    hint_screensaver: "Salvapantallas",
    hint_folder: "Carpeta",
    hint_folder_path: "Ruta de carpeta",
    hint_share_folder: "Compartir carpeta",
    hint_select_target: "Elegir destino",
    hint_open_subfolder: "Abrir subcarpeta",
    hint_up_one_level: "Subir un nivel",
    hint_up_back: "Subir / Atrás",
    hint_back_to_device_id: "Volver a ID",
    hint_pause_enable: "Pausar/Activar",
    hint_add_by_id: "Añadir por ID",
    hint_scan_qr: "Escanear QR",
    hint_apply_sync: "Aplicar y sinc",
    hint_confirm_delete: "Confirmar eliminación",
    hint_confirm_quit: "Confirmar salida",
    hint_create_pick_devices: "Crear y elegir disp.",
    hint_add_remote_device_id: "Añadir ID remoto",
    hint_close_qr_device_id: "Cerrar QR / ID",
    hint_accept_share: "Aceptar carpeta",
    hint_confirm_add: "Confirmar y añadir",
    hint_type_key: "Pulsar tecla",

    // Screen 1: Dashboard (Overview)
    dashboard_title: "Resumen",
    hero_this_device: "Este dispositivo",
    hero_device_name: "PlayStation Vita",
    hero_device_id_label: "ID del dispositivo",
    section_recent_activity: "Actividad reciente",
    msg_no_sync_events: "Sin eventos de sincronización registrados aún.",

    // Screen 2: Shared Folders
    shares_title: "Carpetas compartidas",
    msg_no_shares: "No hay carpetas compartidas configuradas aún",
    msg_no_shares_hint_1: "Pulsa Círculo (Nueva carpeta) para crear una carpeta y vincular un dispositivo,",
    msg_no_shares_hint_2: "o pulsa Resumen -> Sincronizar para importar carpetas compartidas.",

    // Screen 3: Files & Folder Picker
    picker_title_create: "Compartir una carpeta",
    picker_subtitle_create: "Selecciona una carpeta para compartirla o ábrela para explorar",
    picker_title_choose: "Elegir carpeta de destino",
    picker_default_share: "Carpeta predeterminada",

    // Screen 4: Settings & Protocol Log
    settings_title: "Ajustes y registro",
    settings_subtitle: "Pantalla y BEP/1.0",
    settings_sec_display: "PANTALLA Y SALVAPANTALLAS OLED",
    settings_label_colour_theme: "Tema de color",
    settings_desc_theme_dark: "Oscuro — Diseño azul pizarra VitaSend con acento turquesa",
    settings_desc_theme_oled: "OLED — Fondo negro puro #000000 (ahorra batería en PS Vita OLED)",
    settings_saver_auto_hint: "El salvapantallas OLED se activa tras 12s al sincronizar (o botón SELECT).",
    settings_sec_log: "REGISTRO DE PROTOCOLO BEP/1.0 Y TLS 1.3",
    settings_sec_declined: "CARPETAS RECHAZADAS",
    settings_no_declined: "Ninguna · las carpetas ofrecidas por pares siempre se preguntan",

    // Screensaver
    saver_wake_hint: "Toca la pantalla o presiona cualquier botón para reactivar",

    // Modals: Create Share
    modal_create_share_title: "Crear nueva carpeta compartida",
    modal_create_share_subtitle: "Elige una carpeta predefinida o explora ux0: para compartir",
    modal_create_share_browse_sub: "Abrir el explorador interactivo de directorios ux0:",

    // Modals: Manage Devices
    modal_manage_devices_default_title: "Carpeta",
    modal_manage_devices_subtitle_incomplete: "Vincula al menos un dispositivo para crear esta carpeta",
    modal_manage_devices_subtitle: "Marca los dispositivos Syncthing que deben sincronizar esta carpeta",
    modal_manage_devices_scanning: "Buscando pares Syncthing en LAN (UDP 21027 + TCP 22000)...",
    modal_manage_devices_empty: "No se descubrieron dispositivos Syncthing remotos aún.",
    modal_manage_devices_empty_hint: "Pulsa Cuadrado (Añadir por ID) o Triángulo para escanear LAN.",

    // Modals: Add Device
    modal_add_device_title: "Añadir dispositivo remoto por ID",
    modal_add_device_subtitle: "Introduce el ID corto de 7 caracteres o el ID completo de 56 caracteres",
    modal_add_device_example: "ej. MFZWI3D  o  MFZWI3D-BONSGYC-... (toca para escribir)",
    modal_add_device_tip: "Consejo: Solo necesitas los primeros 7 caracteres (ID corto).",
    modal_add_device_placeholder: "Introduce ID corto de 7 car. o completo de 56 car...",
    modal_add_device_name_label: "Nombre del dispositivo (opcional):",
    modal_add_device_name_empty: "(toca para introducir nombre)",
    modal_add_device_confirm: "Añadir dispositivo",

    // Modals: Confirm Delete
    modal_delete_share_title: "¿Eliminar esta carpeta?",
    modal_delete_share_subtitle: "La carpeta dejará de sincronizarse con todos los dispositivos vinculados",
    modal_delete_share_decline_hint: "Las ofertas futuras de pares se rechazarán (deshacer en Ajustes).",

    // Modals: Confirm Quit
    modal_quit_title: "¿Salir de VitaSync?",
    modal_quit_subtitle: "Confirma si deseas cerrar la aplicación y volver a LiveArea",
    modal_quit_syncing_warn: "Hay una sincronización BEP/1.0 en curso.",
    modal_quit_syncing_hint: "Salir ahora interrumpirá las transferencias de archivos activas.",
    modal_quit_idle_ready: "¿Estás seguro de que deseas salir de VitaSync?",
    modal_quit_idle_hint: "Todas las configuraciones y ajustes están guardados en ux0:.",

    // Modals: Device ID
    modal_device_id_title: "ID del dispositivo PS Vita Syncthing",
    modal_device_id_subtitle: "Escanea este código QR en Syncthing o introduce el ID de 56 caracteres",
    modal_device_id_scan_web: "Escanea el QR en la interfaz Web o móvil de Syncthing",
    modal_device_id_dev_name: "Nombre del dispositivo: PlayStation Vita (vita-syncthing)",
    modal_device_id_cert_path: "Certificado de identidad: ux0:data/syncthing/cert.der",
    modal_device_id_short_label: "ID corto (descubrimiento LAN):",
    modal_device_id_full_label: "ID de dispositivo de 56 caracteres:",
    modal_device_id_scan_hint: "En móvil/PC: Syncthing > Dispositivos > Añadir > Escanear QR",

    // Modals: Folder Offer
    modal_folder_offer_title: "Nueva carpeta compartida",
    modal_folder_offer_hint: "Acepta para elegir una carpeta en ux0:. Nada se sincroniza hasta activarla.",

    // Modals: Scan QR
    modal_scan_qr_title: "Escanear código QR del dispositivo",
    modal_scan_qr_subtitle: "Syncthing: Acciones > Mostrar ID, luego apunta la cámara trasera",
};
