import React, { useEffect, useState, useMemo, useRef } from "react";
import ReactDOM from "react-dom/client";
import Papa from "papaparse";
import { SigmaContainer, useLoadGraph, useRegisterEvents, useSigma } from "@react-sigma/core";
import '@react-sigma/core/lib/style.css';
import { MultiDirectedGraph as MultiGraphConstructor } from 'graphology';
import { circular, random, circlepack } from "graphology-layout";
import { createEdgeArrowProgram } from 'sigma/rendering';
import "./style.css"

type Event = {
  i: number;
  time: number;
  node: number;
  event: "up" | "down" | "recv";
  peer: number;
  msg: string;
};

const COLORS = [
  "silver",
  "red",
  "orange",
  "gold",
  "yellowgreen",
  "green"
]

enum Layout {
  circular = "cirulcar",
  random = "random",
  circlepack = "circlepack"
}

const GraphView: React.FC<{ events: Event[], time: number, layout: Layout }> = ({ events, time, layout }) => {
  const loadGraph = useLoadGraph();
  const graph = useMemo(() => new MultiGraphConstructor(), []);
  const [highlightedNode, setHighlightedNode] = useState<string | null>(null);
  const registerEvents = useRegisterEvents()
  const lastTime = useRef<number>(0)

  useEffect(() => {
    let intervalStart
    if (time < lastTime.current) {
      graph.clear()
      intervalStart = 0
    } else {
      intervalStart = lastTime.current
      lastTime.current = time
    }

    const addNodes = (event: Event) => {
      for (const n of [event.node, event.peer]) {
        const id = n.toString();
        if (!graph.hasNode(id)) {
          graph.addNode(id, { label: id })
        }
      }
    }

    const edgeKeys = (event: Event) => {
      const src = event.node.toString()
      const dst = event.peer.toString()
      const key = `${src}->${dst}`;
      return { src, dst, key }
    }

    const filteredEvents = events.filter(e => e.time > intervalStart && e.time <= time)
    for (const event of filteredEvents) {
      addNodes(event)
      if (event.event === "up") {
        const { src, dst, key } = edgeKeys(event);
        if (!graph.hasEdge(key)) {
          graph.addDirectedEdgeWithKey(key, src, dst);
        }
      }
      else if (event.event === "down") {
        const { key } = edgeKeys(event);
        graph.dropEdge(key)
      }
    }

    if (layout === Layout.circular) {
      circular.assign(graph);
    } else if (layout === Layout.random) {
      random.assign(graph)
    } else if (layout === Layout.circlepack) {
      circlepack.assign(graph)
    } else {
      throw new Error("invalid layout: " + layout)
    }

    graph.forEachNode((n) => {
      const deg = graph.degree(n);
      const color = COLORS[Math.min(deg, 5)]
      graph.setNodeAttribute(n, "color", color);
      graph.setNodeAttribute(n, "size", 7);
    });
    graph.forEachEdge((e) => {
      let color
      if (highlightedNode) {
        if (graph.extremities(e).includes(highlightedNode)) {
          color = 'red'
        } else {
          color = 'rgba(0,0,0,0.05)'
        }
      } else {
        color = 'rgba(0,0,0,0.1)'
      }
      graph.setEdgeAttribute(e, "color", color);
      graph.setEdgeAttribute(e, "size", 1);
      graph.setEdgeAttribute(e, "arrow", "both");
    });

    loadGraph(graph);
  }, [graph, time, events, layout, loadGraph, highlightedNode]);

  useEffect(() => {
    registerEvents({
      clickNode: ({ node }) => {
        setHighlightedNode((prev) => (prev === node ? null : node));
      },
    })

  }, [registerEvents])

  return null;
};

const App = () => {
  const [events, setEvents] = useState<Event[]>([]);
  const [time, setTime] = useState(0);
  const [maxTime, setMaxTime] = useState(0);
  const [times, setTimes] = useState<number[]>([]);
  const [layout, setLayout] = useState(Layout.circular)

  useEffect(() => {
    Papa.parse<Event>("/data/GossipMulti-n100-r30.events.0.csv", {
      download: true,
      header: true,
      dynamicTyping: true,
      complete: (results) => {
        const rows = (results.data).filter((r) => r.time);
        setEvents(rows);
        const ts = rows.map((e) => e.time).sort((a, b) => a - b);
        setTimes(ts);
        setMaxTime(Math.max(...ts));
      },
    });
  }, []);

  const currentEvents = useMemo(() => events.filter((e) => e.time === time), [events, time]);
  const nextTime = () => {
    const next = times.find((t) => t > time);
    if (next !== undefined) setTime(next);
  };

  // Sigma settings
  const settings = useMemo(
    () => ({
      allowInvalidContainer: true,
      renderEdgeLabels: true,
      defaultEdgeType: 'straight',
      edgeProgramClasses: {
        straight: createEdgeArrowProgram({ lengthToThicknessRatio: 5, widenessToThicknessRatio: 5 })
      },
    }),
    [],
  );

  return (
    <>
      <div className="app">
        <SigmaContainer style={{ height: "100vh" }} settings={settings}>
          {events.length > 0 && <GraphView events={events} time={time} layout={layout} />}
        </SigmaContainer>
        <div className="sidebar">
          <select onChange={e => setLayout(e.target.value as unknown as Layout)}>
            {Object.keys(Layout).map((key, index) => (
              <option key={index} value={key}>{key}</option>
            ))}
          </select>
          <input
            type="range"
            min={0}
            max={maxTime}
            step={1}
            value={time}
            onChange={(e) => setTime(parseInt(e.target.value))}
          />
          <button onClick={nextTime}>Next</button>
          <div>Time: {time}</div>
          <h3>Events at time</h3>
          {currentEvents.map((e, i) => (
            <div key={i}>
              [{e.time}] {e.node} {e.event} {e.peer}
            </div>
          ))}
        </div>
      </div>
    </>
  );
};

const root = ReactDOM.createRoot(document.getElementById("root")!);
root.render(<App />);
