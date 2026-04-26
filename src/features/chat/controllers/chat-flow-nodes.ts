import { ref } from "vue";

import type { FlowNodeEntry } from "../../../lib/chat-types";

const FLOW_NODE_STORAGE_KEY = "nova:flowNodes";

function loadStoredFlowNodes(): FlowNodeEntry[] {
  const raw = localStorage.getItem(FLOW_NODE_STORAGE_KEY);
  return raw ? (JSON.parse(raw) as FlowNodeEntry[]) : [];
}

export function createFlowNodeStore() {
  const flowNodes = ref<FlowNodeEntry[]>(loadStoredFlowNodes());

  function persist() {
    localStorage.setItem(FLOW_NODE_STORAGE_KEY, JSON.stringify(flowNodes.value));
  }

  function upsertFlowNode(entry: FlowNodeEntry) {
    const idx = flowNodes.value.findIndex((node) => node.nodeId === entry.nodeId);
    if (idx !== -1) {
      flowNodes.value[idx] = entry;
    } else {
      flowNodes.value.push(entry);
    }
    persist();
  }

  function clearFlowNodes() {
    flowNodes.value = [];
    localStorage.removeItem(FLOW_NODE_STORAGE_KEY);
  }

  return {
    clearFlowNodes,
    flowNodes,
    upsertFlowNode,
  };
}
