import React, { useContext, useEffect, useMemo, useRef, useState } from "react";
import Papa from "papaparse";
import {
  SigmaContainer,
  useLoadGraph,
  useRegisterEvents,
} from "@react-sigma/core";
import "@react-sigma/core/lib/style.css";
import { MultiDirectedGraph } from "graphology";
import { circlepack, circular, random } from "graphology-layout";
// import forceAtlas2 from "graphology-layout-forceatlas2";
import { createEdgeArrowProgram } from "sigma/rendering";
import { createNodeBorderProgram } from "@sigma/node-border";

export type SimEvent = {
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
  "green",
];

enum Layout {
  circular = "circular",
  force = "force",
  random = "random",
  circlepack = "circlepack",
}

interface GraphViewProps {
  events: SimEvent[];
  time: number;
  layout: Layout;
  graph: MultiDirectedGraph;
  highlightedNode: string | null;
  setHighlightedNode: React.Dispatch<React.SetStateAction<string | null>>;
}

function GraphView(
  { events, time, layout, graph, highlightedNode, setHighlightedNode }: GraphViewProps
) {
  const loadGraph = useLoadGraph();
  const registerEvents = useRegisterEvents();
  const lastTime = useRef<number>(0);

  useEffect(() => {
    let intervalStart;
    if (time < lastTime.current) {
      graph.clear();
      intervalStart = 0;
    } else {
      intervalStart = lastTime.current;
      lastTime.current = time;
    }

    const addNodes = (event: SimEvent) => {
      for (const n of [event.node, event.peer]) {
        const id = n.toString();
        if (!graph.hasNode(id)) {
          graph.addNode(id, { label: id });
        }
      }
    };

    const edgeKeys = (event: SimEvent) => {
      const src = event.node.toString();
      const dst = event.peer.toString();
      const key = `${src}->${dst}`;
      return { src, dst, key };
    };

    const filteredEvents = events.filter((e) =>
      e.time > intervalStart && e.time <= time
    );
    for (const event of filteredEvents) {
      addNodes(event);
      if (event.event === "up") {
        const { src, dst, key } = edgeKeys(event);
        if (!graph.hasEdge(key)) {
          graph.addDirectedEdgeWithKey(key, src, dst);
        }
      } else if (event.event === "down") {
        const { key } = edgeKeys(event);
        graph.dropEdge(key);
      }
    }

    // const newNodes = graph.filterNodes(node => graph.getNodeAttribute(node, 'isNew'));
    if (layout === Layout.circular) {
      circular.assign(graph);
    } else if (layout === Layout.random) {
      random.assign(graph);
    } else if (layout === Layout.circlepack) {
      circlepack.assign(graph);
      // } else if (layout === Layout.force) {
      //   forceAtlas2.assign(graph, {
      //     // nodes: newNodes,
      //     iterations: 100,
      //     settings: {
      //       linLogMode: false,
      //       outboundAttractionDistribution: false,
      //       adjustSizes: false,
      //       edgeWeightInfluence: 1,
      //     }
      //   });
    } else {
      throw new Error("invalid layout: " + layout);
    }

    graph.forEachNode((n) => {
      const deg = graph.degree(n);
      const color = COLORS[Math.min(deg, 5)];
      graph.setNodeAttribute(n, "color", color);
      if (highlightedNode == n) {
        graph.setNodeAttribute(n, "borderColor", "fuchsia");
        graph.setNodeAttribute(n, "borderSize", 0.3);
        graph.setNodeAttribute(n, "size", 12);
      } else if (highlightedNode && graph.areNeighbors(n, highlightedNode)) {
        graph.setNodeAttribute(n, "borderColor", "fuchia");
        graph.setNodeAttribute(n, "borderSize", 0.15);
        graph.setNodeAttribute(n, "size", 9);
      } else {
        graph.setNodeAttribute(n, "borderSize", 0);
        graph.setNodeAttribute(n, "size", 7);
      }
    });
    graph.forEachEdge((e) => {
      let color;
      if (highlightedNode) {
        if (graph.extremities(e).includes(highlightedNode)) {
          color = "fuchsia";
        } else {
          color = "rgba(0,0,0,0.05)";
        }
      } else {
        color = "rgba(0,0,0,0.1)";
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
    });
  }, [registerEvents, setHighlightedNode]);

  return null;
};

export interface GraphContextType {
  graph: MultiDirectedGraph,
  highlightedNode: string | null
  setHighlightedNode: React.Dispatch<React.SetStateAction<string | null>>;
  events: SimEvent[];
  setEvents: React.Dispatch<React.SetStateAction<SimEvent[]>>;
  time: number;
  setTime: React.Dispatch<React.SetStateAction<number>>;
  layout: Layout;
  setLayout: React.Dispatch<React.SetStateAction<Layout>>;
  maxTime: number;
  setMaxTime: React.Dispatch<React.SetStateAction<number>>;
}

export const GraphContextBase = React.createContext<null | GraphContextType>(null)

export function GraphContext({ children }: { children: React.ReactNode }) {
  const [highlightedNode, setHighlightedNode] = useState<string | null>(null);
  const graph = useMemo(() => new MultiDirectedGraph(), []);
  const [events, setEvents] = useState<SimEvent[]>([]);
  const [time, setTime] = useState(0);
  const [layout, setLayout] = useState(Layout.circular);
  const [maxTime, setMaxTime] = useState(0);
  const value = { graph, time, setTime, layout, setLayout, maxTime, setMaxTime, events, setEvents, highlightedNode, setHighlightedNode }
  return (
    <GraphContextBase.Provider value={value}>
      {children}
    </GraphContextBase.Provider>
  )
}
export function useGraphContext() {
  const ctx = useContext(GraphContextBase)
  function load(events: SimEvent[]) {
    if (!ctx) throw new Error("Missing graph context")
    ctx.setEvents(events)
    const lastTime = events[events.length - 1]?.time || 0
    ctx.setMaxTime(lastTime)
    ctx.setTime(lastTime)
  }
  return {
    ...ctx,
    load
  }
}

export interface GraphProps {
  eventsUrl: string
}

export function Graph() {
  const context = useGraphContext()
  if (!context) return null
  const { graph, highlightedNode, setHighlightedNode, events, time, layout } = context


  // Sigma settings
  const settings = useMemo(
    () => ({
      allowInvalidContainer: true,
      renderEdgeLabels: true,
      defaultEdgeType: "straight",
      edgeProgramClasses: {
        straight: createEdgeArrowProgram({
          lengthToThicknessRatio: 5,
          widenessToThicknessRatio: 5,
        }),
      },
      defaultNodeType: "bordered",
      nodeProgramClasses: {
        bordered: createNodeBorderProgram({
          borders: [
            {
              size: { attribute: "borderSize", defaultValue: 0 },
              color: { attribute: "borderColor" },
            },
            { size: { fill: true }, color: { attribute: "color" } },
          ],
        }),
      },
    }),
    [],
  );

  return (
    <SigmaContainer style={{ height: "100vh" }} settings={settings}>
      {events.length > 0 && (
        <GraphView
          events={events}
          time={time}
          layout={layout}
          graph={graph}
          highlightedNode={highlightedNode}
          setHighlightedNode={setHighlightedNode}
        />
      )}
    </SigmaContainer>
  )
}

export function GraphControls() {
  const context = useGraphContext()
  if (!context) return null
  const { graph, events, highlightedNode, setLayout, time, maxTime, setTime } = context

  const nextTime = events.find(x => x.time > time)?.time || time
  const currentEvents = useMemo(() => events.filter(x => x.time === time), [events, time])

  return (
    <div className="sidebar">
      <select
        onChange={(e) => setLayout(e.target.value as unknown as Layout)}
      >
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
      <button disabled={time === nextTime} onClick={_ => setTime(nextTime)}>Next</button>
      <div>Time: {time}</div>
      <h3>Events at time</h3>
      {currentEvents.map((e, i) => (
        <div key={i}>
          [{e.time}] {e.node} {e.event} {e.peer}
        </div>
      ))}
      {highlightedNode && (
        <NodeDetails graph={graph} node={highlightedNode} />
      )}
    </div>
  )
}

function NodeDetails(
  { graph, node }: { graph: MultiDirectedGraph; node: string },
) {
  const edges = graph.edges(node);
  return (
    <div>
      <h3>node {node}</h3>
      <ul>
        {edges.map((edge) => <li key={edge}>{edge}</li>)}
      </ul>
    </div>
  );
}
