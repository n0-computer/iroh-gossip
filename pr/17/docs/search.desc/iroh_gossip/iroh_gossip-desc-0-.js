searchState.loadedDescShard("iroh_gossip", 0, "Broadcast messages to peers subscribed to a topic\nALPN protocol name\nDefine the gossiping subcommands.\nMetrics for iroh-gossip\nNetworking for the <code>iroh-gossip</code> protocol\nImplementation of the iroh-gossip protocol, as an IO-less …\nProvides a rpc protocol as well as a client for the …\nCommands to manage gossiping.\nSubscribe to a gossip topic\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nRuns the gossip command given the iroh client.\nThe set of nodes that are also part of the gossip swarm to …\nThe raw topic to subscribe to as hex. Needs to be 32 …\nThe topic to subscribe to.\nIf enabled, all gossip events will be printed, including …\nEnum of metrics for the module\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nBroadcast a message for this topic.\nBroadcast a message to all nodes in the swarm\nBroadcast a message to all direct neighbors\nBuilder to configure and construct <code>Gossip</code>.\nSend a gossip message\nA stream of commands for a gossip subscription.\nEvents emitted from a gossip topic with a lagging …\nStream of events for a topic.\nALPN protocol name\nPublish and subscribe on gossiping topics.\nWe received an event.\nEvents emitted from a gossip topic.\nReceiver for gossip events on a topic.\nSender for a gossip topic.\nSubscribed gossip topic.\nJoin this topic and connect to peers.\nOptions for joining a gossip topic.\nConnect to a set of peers\nWe joined the topic with at least one peer.\nWe missed some messages because our <code>GossipReceiver</code> was not …\nA gossip message\nWe dropped direct neighbor in the swarm membership layer …\nWe dropped direct neighbor in the swarm membership layer …\nWe have a new, direct neighbor in the swarm membership …\nWe have a new, direct neighbor in the swarm membership …\nCommands for the gossip protocol\nEvents emitted from the gossip protocol\nLeave this topic and drop all state.\nA gossip message was received for this topic\nWe received a gossip message for this topic.\nThe initial bootstrap nodes\nBroadcast a message to all nodes.\nSends a message to all peers.\nSet the broadcast configuration.\nBroadcast a message to our direct neighbors.\nSends a message to our direct neighbors in the swarm.\nCreates a default <code>Builder</code>, with the endpoint set.\nGet an in-memory gossip client\nThe content of the message\nThe node that delivered the message. This is not the same …\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nHandle an incoming <code>Connection</code>.\nHandle a gossip request from the RPC server.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nReturns true if we are connected to at least one node.\nReturns true if we are connected to at least one node.\nJoin a gossip topic with the default options and wait for …\nJoin a set of peers.\nJoin a gossip topic with options.\nJoin a gossip topic with options and an externally-created …\nWaits until we are connected to at least one node.\nWaits until we are connected to at least one node.\nGet the maximum message size configured for this gossip …\nSets the maximum message size in bytes. By default this is …\nSet the membership configuration.\nLists our current direct neighbors.\nThe scope of the message. This tells us if the message is …\nSpawn a gossip actor and get a handle for it\nSplits <code>self</code> into <code>GossipSender</code> and <code>GossipReceiver</code> parts.\nThe maximum number of messages that can be buffered in a …\nUtilities for iroh-gossip networking\nCreates <code>JoinOptions</code> with the provided bootstrap nodes and …\nA <code>TimerMap</code> with an async method to wait for the next timer …\nReturns the argument unchanged.\nInsert a new entry at the specified instant\nCalls <code>U::from(self)</code>.\nCreate a new timer map\nReads a length prefixed message.\nRead a length-prefixed message and decode as <code>ProtoMessage</code>;\nWait for the next timer to expire and return an iterator …\nWrite a <code>ProtoMessage</code> as a length-prefixed, …\nThe scope to deliver the message to.\nConfiguration for the swarm membership layer\nThis message was received from a direct neighbor that …\nThe message is broadcast only to the immediate neighbors …\nOpaque binary data that is transmitted on messages that …\nThe identifier for a peer.\nConfiguration for the gossip broadcast layer.\nThe broadcast scope of a gossip message.\nThis message was received from the swarm, with a distance …\nThe message is broadcast to all peers in the swarm.\nNumber of hops a <code>ForwardJoin</code> message is propagated until …\nNumber of peers to which active connections are maintained\nGet the peer data as a byte slice.\nHow often the internal caches will be checked for expired …\nDefault values for the HyParView layer\nSensible defaults for the plumtree configuration\nTimeout after which <code>IHave</code> messages are pushed to peers.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nWhen receiving an <code>IHave</code> message, this timeout is …\nThis timeout is registered when sending a <code>Graft</code> message. …\nGet a reference to the contained <code>bytes::Bytes</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nWhether this message was directly received from its …\nDuration for which to keep gossip messages in the internal …\nDuration for which to keep the <code>MessageId</code>s for received …\nTimeout after which a <code>Neighbor</code> request is considered failed\nCreate a new <code>PeerData</code> from a byte buffer.\nThe protocol performs a tree optimization, which promotes …\nNumber of hops a <code>ForwardJoin</code> message is propagated until …\nNumber of peers for which contact information is …\nNumber of active peers to be included in a <code>Shuffle</code> request.\nInterval duration for shuffle requests\nNumber of passive peers to be included in a <code>Shuffle</code> …\nNumber of hops a <code>Shuffle</code> message is propagated until a …\nThe protocol state of the <code>iroh-gossip</code> protocol.\nThis module contains the implementation of the gossiping …\nUtilities used in the protocol implementation\nExecute a command from the application.\nA control message.\nA data message.\nClose the connection to a peer on the network level.\nEmit an event to the application.\nInput event to the protocol state.\nProtocol wire message\nWhether this is a control or data message\nOutput event from the protocol state.\nUpdated peer data\nPeer disconnected on the network level.\nMessage received from the network.\nSchedule a timer. The runtime is responsible for sending …\nSend a message on the network\nThe state of the <code>iroh-gossip</code> protocol.\nA timer to be registered into the runtime\nTrigger a previously scheduled timer.\nThe identifier for a topic\nUpdate the opaque peer data about yourself.\nGet as byte slice.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nCreate from a byte array.\nHandle an <code>InEvent</code>\nCheck if a topic has any active (connected) peers.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nGet the kind of this message\nReturns the maximum message size configured in the gossip …\nGet a reference to the node’s <code>PeerIdentity</code>\nCreate a new protocol state instance.\nGet the encoded size of this message\nGet a reference to the protocol state for a topic.\nGet an iterator for the states of all joined topics.\nGet an iterator of all joined topics.\nBroadcast a message for this topic.\nA command to the protocol state for a particular topic.\nExecute a command from the application.\nProtocol configuration\nThe default maximum size in bytes for a gossip message. …\nClose the connection to a peer on the network level.\nEmit an event to the application.\nAn event to be emitted to the application for a particular …\nA message of the gossip broadcast layer\nA timer for the gossip layer\nA trait for a concrete type to push <code>OutEvent</code>s to.\nInput event to the topic state handler.\nJoin this topic and connect to peers.\nA protocol message for a particular topic\nWe dropped direct neighbor in the swarm membership layer …\nWe have a new, direct neighbor in the swarm membership …\nAn output event from the state handler.\nEmitted when new <code>PeerData</code> was received for a peer.\nPeer disconnected on the network level.\nLeave this topic and drop all state.\nA gossip message was received for this topic\nMessage received from the network.\nSchedule a timer. The runtime is responsible for sending …\nSend a message on the network\nThe topic state maintains the swarm membership and …\nStatistics for the protocol state of a topic\nA message of the swarm membership layer\nA timer for the swarm layer\nA timer to be registered for a particular topic.\nTrigger a previously scheduled timer.\nUpdate the opaque peer data about yourself.\nConfiguration for the gossip broadcast layer\nThe address of your local endpoint.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nGet statistics for the gossip broadcast state\nHandle an incoming event.\nCheck if this topic has any active (connected) peers.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nGet the kind of this message\nMax message size in bytes.\nConfiguration for the swarm membership layer\nNumber of messages received\nNumber of messages sent\nInitialize the local state with the default random number …\nPush an event in the IO container\nPush all events from an iterator into the IO container\nGet stats on how many messages were sent and received\nInitialize the local state with a custom random number …\nA hash map where entries expire after a time\nA <code>BTreeMap</code> with <code>Instant</code> as key. Allows to process expired …\nReturns <code>true</code> if the map contains a value for the specified …\nRemove and return all entries before and equal to <code>from</code>.\nRemove all entries with an expiry instant lower or equal …\nGet the expiration time for an item.\nGet a reference to the earliest entry in the TimerMap.\nReturns the argument unchanged.\nReturns the argument unchanged.\nGet an item from the cache.\nInsert a new entry at the specified instant.\nInsert an item into the cache, marked with an expiration …\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nReturns <code>true</code> if the map contains no elements.\nIterate over all items in the timer map.\nIterate over all items in the cache.\nGet the number of entries in the cache.\nCreate a new, empty TimerMap.\nRemove an entry from the specified instant.\nRemove an item from the cache.\nIroh gossip client.\nThe RPC protocol between client and node\nIroh gossip client.\nType alias for a memory-backed client.\nOptions for subscribing to a gossip topic.\nBootstrap nodes to connect to.\nReturns the argument unchanged.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCreates a new gossip client.\nSubscribes to a gossip topic with default options.\nSubscribes to a gossip topic.\nSubscription capacity.\nThe RPC service type for the gossip protocol\nA request to the node to subscribe to gossip events.\nThe nodes to bootstrap the subscription from\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nReturns the argument unchanged.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nCalls <code>U::from(self)</code>.\nThe capacity of the subscription\nThe topic to subscribe to")