# pallet-tkf-dashboard

Esqueleto de pallet FRAME para Tokfinet.

## Funcionalidad
- Ejemplo básico de almacenamiento ()
- Ejemplo de extrinsic ()

## Uso
1. Añadir al workspace en el Cargo.toml raíz
2. Añadir al runtime con `scripts/add-pallet.sh pallet-tkf-dashboard <índice>`

Preparando todo esto, me ha surgido una duda sobre la monitorizacion, es conveniente crear un pallet-dashboard, o lo hacemos con graphana y prometeus, 
me gustaria ser indpendiente tambien en esto, utilizar las apis, pero majearlo internamente. 
Por lo que se substrate, ya  usa prometheus

Substrate ya expone métricas en Prometheus (por defecto en :9615/metrics).
Con eso puedes enchufar Grafana y montar dashboards muy potentes.

El dilema:
    Si usas Prometheus/Grafana → dependes de herramientas externas, aunque sean estándar.
    Si creas un pallet-dashboard → la red se monitoriza a sí misma, y la info está on-chain.

🔍 Comparación de enfoques
    1. Prometheus + Grafana (clásico)

        ✅ Pros:
            Potente y probado en producción.
            Soporte a alertas, queries complejas, correlaciones históricas.
            No cargas la blockchain con datos de monitorización.
        ❌ Contras:
            Dependencia externa (necesitas desplegar Prometheus y Grafana).
            La red Tokfin “no se auto-describe” — depende de infra off-chain.

    2. Pallet-Dashboard (on-chain)
        ✅ Pros:
            Métricas críticas quedan en on-chain state → cualquiera puede consultar desde API/Web3.
            Más independencia: no dependes de Prometheus.
            Puedes customizar métricas específicas de Tokfin (ejemplo: latencia de consenso, disponibilidad de nodos Orchestra, uso de pools del Exchange).
            Los datos pueden alimentar dashboards internos en el frontend sin necesidad de infra extra.
        ❌ Contras:
            Guardar muchas métricas on-chain consume storage y gas.
            Para series temporales largas, no es eficiente.
            Necesitas decidir qué métricas son críticas y cuáles dejas a Prometheus.

🚀 Propuesta híbrida para Tokfin

    No hace falta elegir blanco o negro, te propongo esto:
    Pallet-Dashboard (ligero)
    Solo expone métricas críticas on-chain, en forma de snapshots:
    nº de nodos activos.
    latencia media de bloques.
    % de disponibilidad de cada nodo en los últimos N bloques.
    métricas de uso en pallets clave (ej: liquidity en Exchange).
    Estas métricas se consultan desde el frontend Tokfin → totalmente independiente.
    Prometheus/Grafana (completo)
    Sigue corriendo por debajo para logs de bajo nivel (CPU, RAM, red, gossip, etc.).
    Lo usamos para debugging avanzado y observabilidad profunda.

📌 En resumen:
    Prometheus/Grafana → lo usamos para operación técnica.
    Pallet-Dashboard → expone un subset en on-chain, para independencia y consulta desde el ecosistema Tokfin.