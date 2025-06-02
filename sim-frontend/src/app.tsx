import { default_config } from "gossip-sim-wasm"
import { ConfigSidebar } from "./config"
import { Graph, GraphContext, GraphControls } from "./graph"
import { SimLoader, SimRunner } from "./runner"
import { useState } from "react"

function App() {
  const [config, setConfig] = useState(() => default_config().config)
  return (
    <GraphContext>
      <div className="flex h-screen">
        <div className="w-80 h-full bg-background border-r border-border p-4 overflow-y-auto">
          <SimLoader defaultUrl="data/GossipMulti-n100-r30.events.0.csv" />
          <SimRunner config={config} />
          <ConfigSidebar config={config} setConfig={setConfig} />
        </div>
        <Graph />
        <div className="w-80 h-full bg-background border-l border-border p-4 overflow-y-auto">
          <GraphControls />
        </div>
      </div>
    </GraphContext>

  )
}

export default App
