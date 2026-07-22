-- Content strategy foundation — 32e orientation officielle : Dev Embarqué / IoT.
--
-- Rationale décision 5 de la stratégie contenu 2027-2028 :
--   Aucune orientation IoT/Embedded parmi les 31 seedées en migration 0088.
--   Sujet stratégique pour Skilluv :
--   - Impact Afrique unique : agriculture connectée (capteurs humidité), santé
--     rurale (glucomètres 4G), énergie solaire, monitoring urbain.
--   - Accessibilité matérielle : ESP32 3€, Arduino 20€, Raspberry Pi 15€ —
--     seul métier "hardware" accessible à un junior africain sans capital.
--   - Différenciation Skilluv massive : aucun bootcamp africain sérieux ne
--     fait de l'embarqué (Andela, ALX, Holberton = 100 % software web/mobile).
--   - Croisement Rust : pousse la ligne Rust dans un nouveau territoire.
--
-- 6 skill_nodes créés (domaine 'code', domain principal, avec parent
-- 'dev-embarque-iot-root' pour la hiérarchie).
--
-- Long terme : flagship potentiel "OpenWeather Africa" (réseau distribué de
-- stations météo ESP32 low-cost coordonné via Skilluv, données open).

-- ═══════════════════════════════════════════════════════════════════
-- 1. Orientation officielle
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO orientations (slug, name, description, primary_domain, secondary_domains, tags, is_curated) VALUES
(
    'dev-embarque-iot',
    'Développeur Embarqué / IoT',
    'Programmation microcontrôleurs (ESP32, Arduino, Rust embedded, MicroPython), capteurs, réseaux basse conso (LoRa, MQTT). Focus impact africain : agriculture connectée, santé rurale, énergie solaire, monitoring urbain. Matériel accessible (< 20€).',
    'code',
    ARRAY['ops', 'ai']::TEXT[],  -- Ops (déploiement terrain) + AI (edge inference)
    ARRAY['hardware', 'iot', 'embedded', 'africa-impact']::TEXT[],
    TRUE
);

-- ═══════════════════════════════════════════════════════════════════
-- 2. Skill nodes IoT
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO skill_nodes (slug, display_name, description, domain, aliases, external_refs, is_skilluv_specific) VALUES
(
    'microcontroller-programming',
    'Programmation microcontrôleurs',
    'ESP32, Arduino, STM32. Firmware bas niveau, GPIO, interruptions, timers. Langages : C/C++, Rust embedded, MicroPython.',
    'code',
    ARRAY['esp32', 'arduino', 'stm32', 'mcu', 'firmware'],
    '{"esp-hal": "https://docs.esp-rs.org/", "arduino": "https://www.arduino.cc/reference/en/", "micropython": "https://docs.micropython.org/"}'::jsonb,
    TRUE
),
(
    'sensor-integration',
    'Intégration de capteurs',
    'I2C, SPI, UART. Lecture capteurs analogiques et digitaux : température (DHT22), humidité (soil moisture), CO2 (MH-Z19), GPS (NEO-6M), accéléromètre (MPU-6050). Calibration et filtrage.',
    'code',
    ARRAY['i2c', 'spi', 'uart', 'analog', 'digital'],
    '{"adafruit-sensors": "https://learn.adafruit.com/adafruit-io-basics-sensors"}'::jsonb,
    TRUE
),
(
    'low-power-networking',
    'Réseaux basse consommation',
    'LoRa/LoRaWAN, Sigfox, NB-IoT. Communication longue portée pour zones rurales sans wifi. Protocoles LPWAN, gateways The Things Network.',
    'code',
    ARRAY['lora', 'lorawan', 'sigfox', 'nbiot', 'lpwan'],
    '{"ttn": "https://www.thethingsnetwork.org/", "meshtastic": "https://meshtastic.org/"}'::jsonb,
    TRUE
),
(
    'industrial-iot-protocols',
    'Protocoles IoT industriels',
    'MQTT, Modbus, OPC-UA. Communication entre capteurs et backend cloud. Brokers Mosquitto, EMQX. Patterns pub/sub, QoS levels.',
    'code',
    ARRAY['mqtt', 'modbus', 'opcua', 'mosquitto'],
    '{"mosquitto": "https://mosquitto.org/", "mqtt-spec": "https://mqtt.org/mqtt-specification/"}'::jsonb,
    FALSE
),
(
    'edge-ai',
    'Edge AI / TinyML',
    'Inférence ML sur microcontrôleur (TensorFlow Lite Micro, edge-impulse). Détection anomalies, classification audio/image sur ESP32-CAM. Croisement code + ai.',
    'code',
    ARRAY['tinyml', 'edge-ml', 'tflite-micro', 'edge-impulse'],
    '{"tflite-micro": "https://www.tensorflow.org/lite/microcontrollers", "edge-impulse": "https://www.edgeimpulse.com/"}'::jsonb,
    TRUE
),
(
    'firmware-security',
    'Sécurité firmware',
    'Secure boot, chiffrement flash, mise à jour OTA signée, protection contre reverse engineering. Croisement code + security.',
    'security',
    ARRAY['secure-boot', 'ota', 'firmware-encryption'],
    '{"esp32-secure-boot": "https://docs.espressif.com/projects/esp-idf/en/latest/esp32/security/"}'::jsonb,
    FALSE
);

-- ═══════════════════════════════════════════════════════════════════
-- 3. Mapping orientation ↔ skills (is_core pour les compétences fondamentales)
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO orientation_skill_map (orientation_id, skill_id, is_core, is_recommended, weight)
SELECT
    (SELECT id FROM orientations WHERE slug = 'dev-embarque-iot'),
    sn.id,
    sn.slug IN ('microcontroller-programming', 'sensor-integration'),  -- is_core
    TRUE,
    CASE sn.slug
        WHEN 'microcontroller-programming' THEN 3.0
        WHEN 'sensor-integration' THEN 2.5
        WHEN 'low-power-networking' THEN 2.0
        WHEN 'industrial-iot-protocols' THEN 1.5
        WHEN 'edge-ai' THEN 1.5
        WHEN 'firmware-security' THEN 1.0
        ELSE 1.0
    END
FROM skill_nodes sn
WHERE sn.slug IN (
    'microcontroller-programming',
    'sensor-integration',
    'low-power-networking',
    'industrial-iot-protocols',
    'edge-ai',
    'firmware-security'
);

COMMENT ON COLUMN orientations.slug IS
    '31 orientations 0088 + 32e dev-embarque-iot (0105) = 32 orientations officielles au démarrage Saison 1 Hello World 2027.';
