TO-DO
1.0(*) añadir token reputación TKFr
1.1(*) añadir test y benchmarks para TKFr
1.2(*) añadir token equity TKFe
1.3(*) añadir test y benchmarks oara TKFe
1.4(*) Realizar test y Actualizar repositorio y Documentar cambios - SI todo OK
------------------------------------------------------------------
2.0(*) Crear rama Especialized-Nodes
2.1(*) Pallet FullNode
		Cargo.toml > full-node = [
			"pallet-master/std",        // main chain
			"pallet-certauth/std",      // did, 
			"pallet-storage/std",       // ipfs, performance
			"pallet-exchange/std",      // treasury, pool, payments
			"pallet-devTeam/std",       // dao developers, srv. clients, api, services
			"pallet-dao-srv/std",       // dao entrerprises, srv, clients, api, services
			"pallet-fundation/std",     // governance, polities
			"pallet-cTeam/std", 		// signer, validator, nominator, collator, master
			"pallet-shareholders/std",  // reparto rewards, batch diario
			]
			[features]
				# ... other features
				cert-auth-node = ["pallet-certauth/std", "pallet-balances/std"]
				storage-node = ["pallet-storage/std"]
				app-srv = ["pallet-operations/std"]
		cargo build --release --features "full-node" < compilar por features
2.2(*) Realizar test, documentar y actualizar directorio 
2.3(*) Crear cuentas de nodos para testnet
2.4() Montar testnet en la nube DO/AWS
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
		Polkadot Parallel Computing :
			Para evitar conflictos, puedes utilizar la funcionalidad de storage de Substrate para que cada hilo tenga su propio espacio de almacenamiento, 
			de modo que las transacciones en un hilo no afecten directamente el estado de otro.
			https://wiki.polkadot.com/learn/learn-elastic-scaling/#:~:text=Polkadot%20also%20provides%20throughput%20boost,2024%20with%20the%20Spammening%20Event.
			We decided to split up the work on elastic scaling into two/three phases. 
			First phase is make it work for parachains which trust their collator set and don't share collations with untrusted machines. With this restriction it is possible to launch elastic/fixed factor scaling without any changes to the candidate receipt. 
			Phase 2 will then actually make changes to the candidate receipt in order for the collator set to be untrusted again. (We put the core index in the candidate commitments, so it is not possible to push the same collation to multiple backing groups). 
			Also phase 2 or potentially phase 3, we actually implement everything that is necessary on the cumulus side for elastic/on-demand scaling vs. fixed factor, where we just buy n cores all the time.
		https://forum.polkadot.network/t/scaling-a-polkadot-evm-parachain/4319
		There isn't a native zkRollup integrated directly into Moonbeam, but the Moonbeam Foundation is exploring zk technology through its zkAuth initiative, which uses 		zero-knowledge cryptography for user authentication, not transaction scaling. Moonbeam itself is an Ethereum-compatible parachain on the Polkadot network, leveraging Polkadot's architecture rather than implementing its own Layer-2 scaling solution like a zkRollup. 
		What is a ZK-Rollup?
			A Layer-2 scaling solution for blockchains that processes transactions off-chain. 
			It bundles many transactions into a single batch and uses zero-knowledge proofs to verify their validity on the main chain. 
			This significantly reduces on-chain computation, lowering transaction costs and increasing throughput. 
			Moonbeam's Approach to ZK Technology 
			zkAuth:
			This is Moonbeam's primary focus on zero-knowledge technology, aiming to enhance user experience by allowing users to log in with Web2 credentials instead of relying on seed phrases.
			Not a Scaling Solution:
			While zkAuth uses ZK cryptography, it is not a zkRollup designed to scale Moonbeam's transaction processing capabilities.
			Moonbeam's Architecture 
			Layer 1 Parachain:
			Moonbeam is a Layer-1 parachain within the Polkadot ecosystem, not a Layer-2 solution on top of an existing L1 like Ethereum.
			Polkadot Architecture:
			It benefits from Polkadot's overall security and network design, providing an environment for existing Ethereum applications to operate with minimal changes.
			In summary, Moonbeam is incorporating advanced zero-knowledge technology for user authentication, but it is not implementing a zkRollup for scaling, as it operates as a distinct Layer-1 parachain within the Polkadot network. 
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