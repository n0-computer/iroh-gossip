(function() {var type_impls = {
"iroh_gossip":[["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-Client%3CC%3E\" class=\"impl\"><a class=\"src rightside\" href=\"src/iroh_gossip/rpc/client.rs.html#47-114\">source</a><a href=\"#impl-Client%3CC%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;C&gt; <a class=\"struct\" href=\"iroh_gossip/rpc/client/struct.Client.html\" title=\"struct iroh_gossip::rpc::client::Client\">Client</a>&lt;C&gt;<div class=\"where\">where\n    C: Connector&lt;<a class=\"struct\" href=\"iroh_gossip/rpc/proto/struct.RpcService.html\" title=\"struct iroh_gossip::rpc::proto::RpcService\">RpcService</a>&gt;,</div></h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.new\" class=\"method\"><a class=\"src rightside\" href=\"src/iroh_gossip/rpc/client.rs.html#52-54\">source</a><h4 class=\"code-header\">pub fn <a href=\"iroh_gossip/rpc/client/struct.Client.html#tymethod.new\" class=\"fn\">new</a>(rpc: RpcClient&lt;<a class=\"struct\" href=\"iroh_gossip/rpc/proto/struct.RpcService.html\" title=\"struct iroh_gossip::rpc::proto::RpcService\">RpcService</a>, C&gt;) -&gt; Self</h4></section></summary><div class=\"docblock\"><p>Creates a new gossip client.</p>\n</div></details><details class=\"toggle method-toggle\" open><summary><section id=\"method.subscribe_with_opts\" class=\"method\"><a class=\"src rightside\" href=\"src/iroh_gossip/rpc/client.rs.html#74-93\">source</a><h4 class=\"code-header\">pub async fn <a href=\"iroh_gossip/rpc/client/struct.Client.html#tymethod.subscribe_with_opts\" class=\"fn\">subscribe_with_opts</a>(\n    &amp;self,\n    topic: <a class=\"struct\" href=\"iroh_gossip/proto/state/struct.TopicId.html\" title=\"struct iroh_gossip::proto::state::TopicId\">TopicId</a>,\n    opts: <a class=\"struct\" href=\"iroh_gossip/rpc/client/struct.SubscribeOpts.html\" title=\"struct iroh_gossip::rpc::client::SubscribeOpts\">SubscribeOpts</a>\n) -&gt; <a class=\"type\" href=\"https://docs.rs/anyhow/1.0.94/anyhow/type.Result.html\" title=\"type anyhow::Result\">Result</a>&lt;(impl Sink&lt;<a class=\"enum\" href=\"iroh_gossip/net/enum.Command.html\" title=\"enum iroh_gossip::net::Command\">SubscribeUpdate</a>, Error = <a class=\"struct\" href=\"https://docs.rs/anyhow/1.0.94/anyhow/struct.Error.html\" title=\"struct anyhow::Error\">Error</a>&gt;, impl Stream&lt;Item = <a class=\"type\" href=\"https://docs.rs/anyhow/1.0.94/anyhow/type.Result.html\" title=\"type anyhow::Result\">Result</a>&lt;<a class=\"enum\" href=\"iroh_gossip/net/enum.Event.html\" title=\"enum iroh_gossip::net::Event\">SubscribeResponse</a>&gt;&gt;)&gt;</h4></section></summary><div class=\"docblock\"><p>Subscribes to a gossip topic.</p>\n<p>Returns a sink to send updates to the topic and a stream of responses.</p>\n<p>Updates are either <a href=\"iroh_gossip/net/enum.Command.html#variant.Broadcast\" title=\"variant iroh_gossip::net::Command::Broadcast\">Broadcast</a>\nor <a href=\"iroh_gossip/net/enum.Command.html#variant.BroadcastNeighbors\" title=\"variant iroh_gossip::net::Command::BroadcastNeighbors\">BroadcastNeighbors</a>.</p>\n<p>Broadcasts are gossiped to the entire swarm, while BroadcastNeighbors are sent to\njust the immediate neighbors of the node.</p>\n<p>Responses are either <a href=\"iroh_gossip/net/enum.Event.html#variant.Gossip\" title=\"variant iroh_gossip::net::Event::Gossip\">Gossip</a> or\n<a href=\"iroh_gossip/net/enum.Event.html#variant.Lagged\" title=\"variant iroh_gossip::net::Event::Lagged\">Lagged</a>.</p>\n<p>Gossip events contain the actual message content, as well as information about the\nimmediate neighbors of the node.</p>\n<p>A Lagged event indicates that the gossip stream has not been consumed quickly enough.\nYou can adjust the buffer size with the <a href=\"iroh_gossip/rpc/client/struct.SubscribeOpts.html#structfield.subscription_capacity\" title=\"field iroh_gossip::rpc::client::SubscribeOpts::subscription_capacity\"><code>SubscribeOpts::subscription_capacity</code></a> option.</p>\n</div></details><details class=\"toggle method-toggle\" open><summary><section id=\"method.subscribe\" class=\"method\"><a class=\"src rightside\" href=\"src/iroh_gossip/rpc/client.rs.html#96-113\">source</a><h4 class=\"code-header\">pub async fn <a href=\"iroh_gossip/rpc/client/struct.Client.html#tymethod.subscribe\" class=\"fn\">subscribe</a>(\n    &amp;self,\n    topic: impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/convert/trait.Into.html\" title=\"trait core::convert::Into\">Into</a>&lt;<a class=\"struct\" href=\"iroh_gossip/proto/state/struct.TopicId.html\" title=\"struct iroh_gossip::proto::state::TopicId\">TopicId</a>&gt;,\n    bootstrap: impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/iter/traits/collect/trait.IntoIterator.html\" title=\"trait core::iter::traits::collect::IntoIterator\">IntoIterator</a>&lt;Item = impl <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/convert/trait.Into.html\" title=\"trait core::convert::Into\">Into</a>&lt;NodeId&gt;&gt;\n) -&gt; <a class=\"type\" href=\"https://docs.rs/anyhow/1.0.94/anyhow/type.Result.html\" title=\"type anyhow::Result\">Result</a>&lt;(impl Sink&lt;<a class=\"enum\" href=\"iroh_gossip/net/enum.Command.html\" title=\"enum iroh_gossip::net::Command\">SubscribeUpdate</a>, Error = <a class=\"struct\" href=\"https://docs.rs/anyhow/1.0.94/anyhow/struct.Error.html\" title=\"struct anyhow::Error\">Error</a>&gt;, impl Stream&lt;Item = <a class=\"type\" href=\"https://docs.rs/anyhow/1.0.94/anyhow/type.Result.html\" title=\"type anyhow::Result\">Result</a>&lt;<a class=\"enum\" href=\"iroh_gossip/net/enum.Event.html\" title=\"enum iroh_gossip::net::Event\">SubscribeResponse</a>&gt;&gt;)&gt;</h4></section></summary><div class=\"docblock\"><p>Subscribes to a gossip topic with default options.</p>\n</div></details></div></details>",0,"iroh_gossip::rpc::client::MemClient"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-Clone-for-Client%3CC%3E\" class=\"impl\"><a class=\"src rightside\" href=\"src/iroh_gossip/rpc/client.rs.html#20\">source</a><a href=\"#impl-Clone-for-Client%3CC%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;C: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html\" title=\"trait core::clone::Clone\">Clone</a>&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html\" title=\"trait core::clone::Clone\">Clone</a> for <a class=\"struct\" href=\"iroh_gossip/rpc/client/struct.Client.html\" title=\"struct iroh_gossip::rpc::client::Client\">Client</a>&lt;C&gt;</h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.clone\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/iroh_gossip/rpc/client.rs.html#20\">source</a><a href=\"#method.clone\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#tymethod.clone\" class=\"fn\">clone</a>(&amp;self) -&gt; <a class=\"struct\" href=\"iroh_gossip/rpc/client/struct.Client.html\" title=\"struct iroh_gossip::rpc::client::Client\">Client</a>&lt;C&gt;</h4></section></summary><div class='docblock'>Returns a copy of the value. <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#tymethod.clone\">Read more</a></div></details><details class=\"toggle method-toggle\" open><summary><section id=\"method.clone_from\" class=\"method trait-impl\"><span class=\"rightside\"><span class=\"since\" title=\"Stable since Rust version 1.0.0\">1.0.0</span> · <a class=\"src\" href=\"https://doc.rust-lang.org/nightly/src/core/clone.rs.html#169\">source</a></span><a href=\"#method.clone_from\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#method.clone_from\" class=\"fn\">clone_from</a>(&amp;mut self, source: <a class=\"primitive\" href=\"https://doc.rust-lang.org/nightly/std/primitive.reference.html\">&amp;Self</a>)</h4></section></summary><div class='docblock'>Performs copy-assignment from <code>source</code>. <a href=\"https://doc.rust-lang.org/nightly/core/clone/trait.Clone.html#method.clone_from\">Read more</a></div></details></div></details>","Clone","iroh_gossip::rpc::client::MemClient"],["<details class=\"toggle implementors-toggle\" open><summary><section id=\"impl-Debug-for-Client%3CC%3E\" class=\"impl\"><a class=\"src rightside\" href=\"src/iroh_gossip/rpc/client.rs.html#20\">source</a><a href=\"#impl-Debug-for-Client%3CC%3E\" class=\"anchor\">§</a><h3 class=\"code-header\">impl&lt;C: <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/fmt/trait.Debug.html\" title=\"trait core::fmt::Debug\">Debug</a>&gt; <a class=\"trait\" href=\"https://doc.rust-lang.org/nightly/core/fmt/trait.Debug.html\" title=\"trait core::fmt::Debug\">Debug</a> for <a class=\"struct\" href=\"iroh_gossip/rpc/client/struct.Client.html\" title=\"struct iroh_gossip::rpc::client::Client\">Client</a>&lt;C&gt;</h3></section></summary><div class=\"impl-items\"><details class=\"toggle method-toggle\" open><summary><section id=\"method.fmt\" class=\"method trait-impl\"><a class=\"src rightside\" href=\"src/iroh_gossip/rpc/client.rs.html#20\">source</a><a href=\"#method.fmt\" class=\"anchor\">§</a><h4 class=\"code-header\">fn <a href=\"https://doc.rust-lang.org/nightly/core/fmt/trait.Debug.html#tymethod.fmt\" class=\"fn\">fmt</a>(&amp;self, f: &amp;mut <a class=\"struct\" href=\"https://doc.rust-lang.org/nightly/core/fmt/struct.Formatter.html\" title=\"struct core::fmt::Formatter\">Formatter</a>&lt;'_&gt;) -&gt; <a class=\"type\" href=\"https://doc.rust-lang.org/nightly/core/fmt/type.Result.html\" title=\"type core::fmt::Result\">Result</a></h4></section></summary><div class='docblock'>Formats the value using the given formatter. <a href=\"https://doc.rust-lang.org/nightly/core/fmt/trait.Debug.html#tymethod.fmt\">Read more</a></div></details></div></details>","Debug","iroh_gossip::rpc::client::MemClient"]]
};if (window.register_type_impls) {window.register_type_impls(type_impls);} else {window.pending_type_impls = type_impls;}})()