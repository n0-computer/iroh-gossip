import React, { useMemo } from "react";
import { cn } from "./lib/utils";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"

// TypeScript types based on the Rust structs
export interface MembershipConfig {
  active_view_capacity: number;
  passive_view_capacity: number;
  active_random_walk_length: number;
  passive_random_walk_length: number;
  shuffle_random_walk_length: number;
  shuffle_active_view_count: number;
  shuffle_passive_view_count: number;
  shuffle_interval: string; // duration
  neighbor_request_timeout: string; // duration
}

export interface BroadcastConfig {
  graft_timeout_1: string; // duration
  graft_timeout_2: string; // duration
  dispatch_timeout: string; // duration
  optimization_threshold: number; // Round (integer)
  message_cache_retention: string; // duration
  message_id_retention: string; // duration
  cache_evict_interval: string; // duration
}

export type LatencyConfig =
  | { type: "static"; duration: string } // duration
  | { type: "dynamic"; min: string; max: string }; // duration

export type ProtoConfig = {
  membership: MembershipConfig
  broadcast: BroadcastConfig,
  max_message_size: number
}

export type Config = {
  latency: LatencyConfig,
  proto: ProtoConfig
}

// shadcn UI components
const Button = React.forwardRef<
  HTMLButtonElement,
  React.ButtonHTMLAttributes<HTMLButtonElement>
>(({ className, ...props }, ref) => (
  <button
    className={cn(
      "inline-flex items-center justify-center whitespace-nowrap rounded-md text-sm font-medium ring-offset-background transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 bg-primary text-primary-foreground hover:bg-primary/90 h-10 px-4 py-2",
      className,
    )}
    ref={ref}
    {...props}
  />
));

const Input = React.forwardRef<
  HTMLInputElement,
  React.InputHTMLAttributes<HTMLInputElement>
>(({ className, type, ...props }, ref) => (
  <input
    type={type}
    className={cn(
      "flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
      className,
    )}
    ref={ref}
    {...props}
  />
));

const Label = React.forwardRef<
  React.ElementRef<"label">,
  React.ComponentPropsWithoutRef<"label">
>(({ className, ...props }, ref) => (
  <label
    ref={ref}
    className={cn(
      "text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70",
      className,
    )}
    {...props}
  />
));

const Select = React.forwardRef<
  HTMLSelectElement,
  React.SelectHTMLAttributes<HTMLSelectElement>
>(({ className, ...props }, ref) => (
  <select
    className={cn(
      "flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
      className,
    )}
    ref={ref}
    {...props}
  />
));

// Form components
export const SwarmConfigForm: React.FC<{
  onSubmit: (config: MembershipConfig) => void;
  config: MembershipConfig;
}> = ({ onSubmit, config }) => {

  const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const formData = new FormData(e.currentTarget);
    const newConfig: MembershipConfig = {
      active_view_capacity: Number(formData.get("active_view_capacity")),
      passive_view_capacity: Number(formData.get("passive_view_capacity")),
      active_random_walk_length: Number(
        formData.get("active_random_walk_length"),
      ),
      passive_random_walk_length: Number(
        formData.get("passive_random_walk_length"),
      ),
      shuffle_random_walk_length: Number(
        formData.get("shuffle_random_walk_length"),
      ),
      shuffle_active_view_count: Number(
        formData.get("shuffle_active_view_count"),
      ),
      shuffle_passive_view_count: Number(
        formData.get("shuffle_passive_view_count"),
      ),
      shuffle_interval: formData.get("shuffle_interval") as string,
      neighbor_request_timeout: formData.get("neighbor_request_timeout") as string,
    };
    onSubmit(newConfig);
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor="active_view_capacity">Active View Capacity</Label>
        <Input
          id="active_view_capacity"
          name="active_view_capacity"
          type="number"
          defaultValue={config.active_view_capacity}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="passive_view_capacity">Passive View Capacity</Label>
        <Input
          id="passive_view_capacity"
          name="passive_view_capacity"
          type="number"
          defaultValue={config.passive_view_capacity}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="active_random_walk_length">
          Active Random Walk Length
        </Label>
        <Input
          id="active_random_walk_length"
          name="active_random_walk_length"
          type="number"
          defaultValue={config.active_random_walk_length}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="passive_random_walk_length">
          Passive Random Walk Length
        </Label>
        <Input
          id="passive_random_walk_length"
          name="passive_random_walk_length"
          type="number"
          defaultValue={config.passive_random_walk_length}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="shuffle_random_walk_length">
          Shuffle Random Walk Length
        </Label>
        <Input
          id="shuffle_random_walk_length"
          name="shuffle_random_walk_length"
          type="number"
          defaultValue={config.shuffle_random_walk_length}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="shuffle_active_view_count">
          Shuffle Active View Count
        </Label>
        <Input
          id="shuffle_active_view_count"
          name="shuffle_active_view_count"
          type="number"
          defaultValue={config.shuffle_active_view_count}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="shuffle_passive_view_count">
          Shuffle Passive View Count
        </Label>
        <Input
          id="shuffle_passive_view_count"
          name="shuffle_passive_view_count"
          type="number"
          defaultValue={config.shuffle_passive_view_count}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="shuffle_interval">Shuffle Interval (ms)</Label>
        <Input
          id="shuffle_interval"
          name="shuffle_interval"
          type="text"
          defaultValue={config.shuffle_interval}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="neighbor_request_timeout">
          Neighbor Request Timeout (ms)
        </Label>
        <Input
          id="neighbor_request_timeout"
          name="neighbor_request_timeout"
          type="text"
          defaultValue={config.neighbor_request_timeout}
        />
      </div>

      <Button type="submit" className="w-full">Update Swarm Config</Button>
    </form>
  );
};

export const GossipConfigForm: React.FC<{
  onSubmit: (config: BroadcastConfig) => void;
  config: BroadcastConfig;
}> = ({ onSubmit, config = {} }) => {
  const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const formData = new FormData(e.currentTarget);
    const newConfig: BroadcastConfig = {
      graft_timeout_1: formData.get("graft_timeout_1") as string,
      graft_timeout_2: formData.get("graft_timeout_2") as string,
      dispatch_timeout: formData.get("dispatch_timeout") as string,
      optimization_threshold: Number(formData.get("optimization_threshold")),
      message_cache_retention: formData.get("message_cache_retention") as string,
      message_id_retention: formData.get("message_id_retention") as string,
      cache_evict_interval: formData.get("cache_evict_interval") as string,
    };
    onSubmit(newConfig);
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor="graft_timeout_1">Graft Timeout 1 (ms)</Label>
        <Input
          id="graft_timeout_1"
          name="graft_timeout_1"
          type="text"
          defaultValue={config.graft_timeout_1}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="graft_timeout_2">Graft Timeout 2 (ms)</Label>
        <Input
          id="graft_timeout_2"
          name="graft_timeout_2"
          type="text"
          defaultValue={config.graft_timeout_2}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="dispatch_timeout">Dispatch Timeout (ms)</Label>
        <Input
          id="dispatch_timeout"
          name="dispatch_timeout"
          type="text"
          defaultValue={config.dispatch_timeout}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="optimization_threshold">Optimization Threshold</Label>
        <Input
          id="optimization_threshold"
          name="optimization_threshold"
          type="number"
          defaultValue={config.optimization_threshold}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="message_cache_retention">
          Message Cache Retention (ms)
        </Label>
        <Input
          id="message_cache_retention"
          name="message_cache_retention"
          type="text"
          defaultValue={config.message_cache_retention}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="message_id_retention">Message ID Retention (ms)</Label>
        <Input
          id="message_id_retention"
          name="message_id_retention"
          type="text"
          defaultValue={config.message_id_retention}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor="cache_evict_interval">Cache Evict Interval (ms)</Label>
        <Input
          id="cache_evict_interval"
          name="cache_evict_interval"
          type="text"
          defaultValue={config.cache_evict_interval}
        />
      </div>

      <Button type="submit" className="w-full">Update Gossip Config</Button>
    </form>
  );
};

export const LatencyConfigForm: React.FC<{
  onSubmit: (config: LatencyConfig) => void;
  config: LatencyConfig;
}> = ({ onSubmit, config }) => {
  const [latencyType, setLatencyType] = React.useState<"static" | "dynamic">(
    config.type,
  );

  const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const formData = new FormData(e.currentTarget);

    let newConfig: LatencyConfig;
    if (latencyType === "static") {
      newConfig = {
        type: "static",
        duration: formData.get("duration") as string,
      };
    } else {
      newConfig = {
        type: "dynamic",
        min: formData.get("min") as string,
        max: formData.get("max") as string,
      };
    }
    onSubmit(newConfig);
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor="latency_type">Latency Type</Label>
        <Select
          id="latency_type"
          name="latency_type"
          value={latencyType}
          onChange={(e) =>
            setLatencyType(e.target.value as "static" | "dynamic")}
        >
          <option value="static">Static</option>
          <option value="dynamic">Dynamic</option>
        </Select>
      </div>

      {latencyType === "static"
        ? (
          <div className="space-y-2">
            <Label htmlFor="duration">Duration (ms)</Label>
            <Input
              id="duration"
              name="duration"
              type="text"
              defaultValue={config.type === "static"
                ? config.duration
                : "100ms"}
            />
          </div>
        )
        : (
          <>
            <div className="space-y-2">
              <Label htmlFor="min">Min Duration (ms)</Label>
              <Input
                id="min"
                name="min"
                type="text"
                defaultValue={config.type === "dynamic"
                  ? config.min
                  : "50ms"}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="max">Max Duration (ms)</Label>
              <Input
                id="max"
                name="max"
                type="text"
                defaultValue={config.type === "dynamic"
                  ? config.max
                  : "200ms"}
              />
            </div>
          </>
        )}

      <Button type="submit" className="w-full">Update Latency Config</Button>
    </form>
  );
};

export function ConfigSidebar({ config, setConfig }: {
  config: Config, setConfig:
  React.Dispatch<React.SetStateAction<Config>>;
}) {
  const handleSwarmConfigSubmit = (membership: MembershipConfig) => {
    config = { ...config, proto: { ...config.proto, membership } }
    setConfig(config)
  };

  const handleGossipConfigSubmit = (broadcast: BroadcastConfig) => {
    config = { ...config, proto: { ...config.proto, broadcast } }
    setConfig(config)
  };

  const handleLatencyConfigSubmit = (latency: LatencyConfig) => {
    config = { ...config, latency }
    setConfig(config)
  };

  return (
    <div>
      <div className="space-y-6">
        <Collapsible>
          <CollapsibleTrigger>
            <h3 className="text-lg font-semibold">Swarm Configuration</h3>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <SwarmConfigForm onSubmit={handleSwarmConfigSubmit} config={config.proto.membership} />
          </CollapsibleContent>
        </Collapsible>
        <Collapsible>
          <CollapsibleTrigger>
            <h3 className="text-lg font-semibold">Gossip Configuration</h3>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <GossipConfigForm onSubmit={handleGossipConfigSubmit} config={config.proto.broadcast} />
          </CollapsibleContent>
        </Collapsible>
        <Collapsible>
          <CollapsibleTrigger>
            <h3 className="text-lg font-semibold">Latency Configuration</h3>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <LatencyConfigForm onSubmit={handleLatencyConfigSubmit} config={config.latency} />
          </CollapsibleContent>
        </Collapsible>
      </div>
    </div>
  );
};
