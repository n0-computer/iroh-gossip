import { Button } from "./components/ui/button";

import { run_simulation, default_config } from "gossip-sim-wasm"
import type { Config } from "./config";
import { useState } from "react";
import { useGraphContext, type SimEvent } from "./graph";
import Papa from "papaparse";

export function SimLoader({ defaultUrl }: { defaultUrl: string }) {
  const context = useGraphContext()
  const [url, setUrl] = useState(defaultUrl)

  function onLoadClick() {
    Papa.parse<SimEvent>(url, {
      download: true,
      header: true,
      dynamicTyping: true,
      complete: (results) => {
        const events = results.data.filter((r) => r.time);
        context.load(events)
      },
    });

  }
  return (
    <div className="pb-8">
      <h3>Load simulation</h3>
      <input value={url} onChange={e => setUrl(e.target.value)} />
      <button onClick={_ => onLoadClick()}>Load</button>
    </div>
  )
}

export function SimRunner({ config }: { config: Config }) {
  const context = useGraphContext()
  const [seed, setSeed] = useState(0)
  const [peers, setPeers] = useState(10)
  function onRunClick() {
    const simConfig = {
      seed,
      peers,
      config
    }
    const allEvents = run_simulation(simConfig)
    console.log("simulator events", allEvents);
    const events = allEvents.events.filter(e => e.time)
    if (!context) return
    context.load(events)
  }
  return (
    <div className="pb-8">
      <h3>Run simulation</h3>
      <label htmlFor="peers">Number of nodes</label>
      <input name="peers" type="number" value={peers} onChange={e => setPeers(Number(e.target.value))} />
      <br />
      <label htmlFor="peers">RNG seed</label>
      <input name="seed" type="number" value={seed} onChange={e => setSeed(Number(e.target.value))} />
      <br />
      <Button onClick={onRunClick}>Run</Button>

    </div>
  )

}
