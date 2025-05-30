import React, { useEffect, useState, useMemo } from "react";
import ReactDOM from "react-dom/client";
import Papa from "papaparse";
import { SigmaContainer, useLoadGraph, useRegisterEvents, useSigma } from "@react-sigma/core";
import '@react-sigma/core/lib/style.css';
import Graph from "graphology";
import { MultiDirectedGraph as MultiGraphConstructor } from 'graphology';
import { circular, random } from "graphology-layout";
import { EdgeArrowProgram, createEdgeArrowHeadProgram, createEdgeArrowProgram } from 'sigma/rendering';

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

const GraphView: React.FC<{ events: Event[]; time: number }> = ({ events, time }) => {
  const sigma = useSigma();
  const loadGraph = useLoadGraph();
  const graph = useMemo(() => new MultiGraphConstructor(), []);
  const [highlightedNode, setHighlightedNode] = useState<string | null>(null);
  const registerEvents = useRegisterEvents()

  const activeEdges = useMemo(() => {
    const upMap = new Map<string, boolean>();
    events
      .filter((e) => e.time <= time && e.event !== "recv")
      .forEach((e) => {
        const key = `${e.node}->${e.peer}`;
        if (e.event === "up") upMap.set(key, true);
        else if (e.event === "down") upMap.delete(key);
      });
    return new Set(upMap.keys());
  }, [events, time]);

  useEffect(() => {
    graph.clear();
    const nodesSet = new Set<number>();
    events.forEach((e) => {
      nodesSet.add(e.node);
      nodesSet.add(e.peer);
    });
    nodesSet.forEach((id) => {
      graph.addNode(id.toString(), { label: id.toString() });
    });
    activeEdges.forEach((key) => {
      const [src, dst] = key.split("->");
      if (!graph.hasEdge(src, dst)) graph.addDirectedEdgeWithKey(key, src, dst);
    });

    circular.assign(graph);
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
  }, [graph, events, activeEdges, loadGraph, highlightedNode]);

  useEffect(() => {
    registerEvents({
      clickNode: ({ node }) => {
        console.log("clicked", node)
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

  useEffect(() => {
    Papa.parse<Event>("/data/GossipMulti-n100-r30.events.0.csv", {
      download: true,
      header: true,
      dynamicTyping: true,
      complete: (results) => {
        const rows = (results.data as any[]).filter((r) => r.time) as Event[];
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
        // straight: EdgeArrowProgram,
        // curved: EdgeCurveProgram,
      },
    }),
    [],
  );

  return (
    <>
      <SigmaContainer style={{ height: "100vh" }} settings={settings}>
        {events.length > 0 && <GraphView events={events} time={time} />}
      </SigmaContainer>
      <div style={{ position: "fixed", bottom: 10, left: "50%", transform: "translateX(-50%)" }}>
        <input
          type="range"
          min={0}
          max={maxTime}
          step={1}
          value={time}
          onChange={(e) => setTime(parseInt(e.target.value))}
        />
        <button onClick={nextTime}>Next</button>
        <span style={{ marginLeft: 10 }}>Time: {time}</span>
      </div>
      <div
        style={{
          position: "fixed",
          bottom: 10,
          right: 10,
          background: "white",
          padding: 5,
          maxHeight: 150,
          overflowY: "auto",
          fontSize: 12,
        }}
      >
        {currentEvents.map((e, i) => (
          <div key={i}>
            [{e.time}] {e.node} {e.event} {e.peer}
          </div>
        ))}
      </div>
    </>
  );
};

const root = ReactDOM.createRoot(document.getElementById("root")!);
root.render(<App />);
