TO-DO
1.0(*) añadir token reputación TKFr
1.1(+) añadir test y benchmarks para TKFr
1.2() añadir token equity TKFe
1.3() añadir test y benchmarks oara TKFe
1.4() Realizar test y Actualizar repositorio y Documentar cambios - SI todo OK
------------------------------------------------------------------
2.0() Crear rama Especialized-Nodes
2.1() Pallet FullNode
		Cargo.toml > full-node = [
			"pallet-master/std",        // main chain
			"pallet-certauth/std",      // did, 
			"pallet-storage/std",       // ipfs, performance
			"pallet-exchange/std",      // treasury, pool, payments
			"pallet-app-srv/std",       // srv. clients, api, services
			"pallet-dao-srv/std",       // srv, clients, api, services
			"pallet-fundation/std",     // governance, poloties
			"pallet-ConsensusTeam/std", // signer, validator, nominator, collator, master
			]
			[features]
				# ... other features
				cert-auth-node = ["pallet-certauth/std", "pallet-balances/std"]
				storage-node = ["pallet-storage/std"]
				app-srv = ["pallet-operations/std"]
		cargo build --release --features "full-node" < compilar por features
2.2() Realizar test, documentar y actualizar directorio 
------------------------------------------------------------------
3.0() Crear Rama FixBlock - Tamaño fijo de bloque (Mina Network based)
3.1() Pallet para la limitación del tamaño: 
		Puedes crear un pallet de propósito específico que limite el tamaño de los bloques. 
		El pallet-transaction-storage de Substrate ya ofrece funcionalidades para gestionar las transacciones en los bloques. 
		Puedes configurarlo para que, en lugar de almacenar todas las transacciones, solo guarde un hash o un sumario de ellas, imitando así el concepto de un estado sucinto.
		Limitar la carga de datos: 
			En el runtime de tu proyecto, puedes usar la configuración de Substrate para imponer un límite estricto en la cantidad de datos que pueden incluirse en un bloque. 
			Esto forzaría a que solo se incluyan las transacciones más importantes o un resumen de estas, tal como lo hace Mina.
3.2()	Relalizar test, doumentar y actualizar repositorio.
------------------------------------------------------------------
4.0() Crear Rama ParallelProcess - procesamiento paralelo (Massa Network based)
4.1() Pallet para el procesamiento de hilos: 
		Puedes crear un pallet que introduzca el concepto de "hilos de transacciones". Este pallet podría:
		Asignar un identificador de hilo a cada transacción.
		Procesar solo las transacciones de un hilo específico en un bloque determinado. Esto simularía el procesamiento en paralelo.
		Gestión del estado dividida: 
			Para evitar conflictos, puedes utilizar la funcionalidad de storage de Substrate para que cada hilo tenga su propio espacio de almacenamiento, 
			de modo que las transacciones en un hilo no afecten directamente el estado de otro.
4.2() Relalizar test, doumentar y actualizar repositorio.
------------------------------------------------------------------
5.0() Crear Rama Reputation
5.1() Implementar un sistema de reputación básico (TKFr):
		Crear un pallet de reputación: En lugar de una lógica de consenso compleja, crea un pallet simple llamado, por ejemplo, pallet-reputation.
		Definir la lógica de ganar reputación: Este pallet podría tener una función básica que permita a un usuario ganar una pequeña cantidad de TKFr (por ejemplo, mint_reputation(account, amount)). 
			Esta función podría ser llamada por un administrador por ahora.
		Definir la lógica de perder reputación: El pallet también debería tener una función para reducir el saldo de TKFr de una cuenta si no ha participado activamente.
		Objetivo del MVP: Demostrar que el token TKFr tiene un mecanismo de emisión y un propósito, lo que establece la base para la lógica de consenso futura.
5.2() Relalizar test, doumentar y actualizar repositorio.
-----------------------------------------------------------
6.0() Crear Rama WCN Social Club
6.1() Clone de Instagram con funciones  básicas, mensaje, leer, grupo (Proramar celdas independientes para cada funcionalidad)
		https://github.com/GabrielSalangsang013/ts-full-stack-instagram-clone
6.2() SmartContract para el clon.
6.3() Test de funcionamiento y de estress 