(function() {
    var implementors = Object.fromEntries([["iroh_gossip",[["impl&lt;'de&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"enum\" href=\"iroh_gossip/net/enum.Command.html\" title=\"enum iroh_gossip::net::Command\">Command</a>"],["impl&lt;'de&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"enum\" href=\"iroh_gossip/net/enum.Event.html\" title=\"enum iroh_gossip::net::Event\">Event</a>"],["impl&lt;'de&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"enum\" href=\"iroh_gossip/net/enum.GossipEvent.html\" title=\"enum iroh_gossip::net::GossipEvent\">GossipEvent</a>"],["impl&lt;'de&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"enum\" href=\"iroh_gossip/proto/enum.DeliveryScope.html\" title=\"enum iroh_gossip::proto::DeliveryScope\">DeliveryScope</a>"],["impl&lt;'de&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"enum\" href=\"iroh_gossip/proto/enum.Scope.html\" title=\"enum iroh_gossip::proto::Scope\">Scope</a>"],["impl&lt;'de&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"enum\" href=\"iroh_gossip/rpc/proto/enum.Request.html\" title=\"enum iroh_gossip::rpc::proto::Request\">Request</a>"],["impl&lt;'de&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"enum\" href=\"iroh_gossip/rpc/proto/enum.Response.html\" title=\"enum iroh_gossip::rpc::proto::Response\">Response</a>"],["impl&lt;'de&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"struct\" href=\"iroh_gossip/net/struct.JoinOptions.html\" title=\"struct iroh_gossip::net::JoinOptions\">JoinOptions</a>"],["impl&lt;'de&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"struct\" href=\"iroh_gossip/net/struct.Message.html\" title=\"struct iroh_gossip::net::Message\">Message</a>"],["impl&lt;'de&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"struct\" href=\"iroh_gossip/proto/state/struct.TopicId.html\" title=\"struct iroh_gossip::proto::state::TopicId\">TopicId</a>"],["impl&lt;'de&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"struct\" href=\"iroh_gossip/proto/struct.PeerData.html\" title=\"struct iroh_gossip::proto::PeerData\">PeerData</a>"],["impl&lt;'de&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"struct\" href=\"iroh_gossip/rpc/proto/struct.SubscribeRequest.html\" title=\"struct iroh_gossip::rpc::proto::SubscribeRequest\">SubscribeRequest</a>"],["impl&lt;'de, PI&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"enum\" href=\"iroh_gossip/proto/topic/enum.Event.html\" title=\"enum iroh_gossip::proto::topic::Event\">Event</a>&lt;PI&gt;<div class=\"where\">where\n    PI: <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt;,</div>"],["impl&lt;'de, PI&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"enum\" href=\"iroh_gossip/proto/topic/enum.Message.html\" title=\"enum iroh_gossip::proto::topic::Message\">Message</a>&lt;PI&gt;<div class=\"where\">where\n    PI: <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt;,</div>"],["impl&lt;'de, PI&gt; <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt; for <a class=\"struct\" href=\"iroh_gossip/proto/state/struct.Message.html\" title=\"struct iroh_gossip::proto::state::Message\">Message</a>&lt;PI&gt;<div class=\"where\">where\n    PI: <a class=\"trait\" href=\"https://docs.rs/serde/1.0.216/serde/de/trait.Deserialize.html\" title=\"trait serde::de::Deserialize\">Deserialize</a>&lt;'de&gt;,</div>"]]]]);
    if (window.register_implementors) {
        window.register_implementors(implementors);
    } else {
        window.pending_implementors = implementors;
    }
})()
//{"start":57,"fragment_lengths":[5275]}