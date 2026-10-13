import { useEffect, useState } from "react";

import type { Asset, Device } from "../lib/browser-api";

type PowerMonitorTreeProps = {
  assets: Asset[];
  devices: Device[];
  isOverview: boolean;
  onSelectAsset(assetId: string): void;
  onSelectDevice(deviceId: string): void;
  onSelectHome(): void;
  selectedAssetId: string | null;
  selectedDeviceId: string | null;
};

export function PowerMonitorTree({
  assets,
  devices,
  isOverview,
  onSelectAsset,
  onSelectDevice,
  onSelectHome,
  selectedAssetId,
  selectedDeviceId,
}: PowerMonitorTreeProps) {
  const [expandedAssetIds, setExpandedAssetIds] = useState<Set<string>>(() => new Set());
  const [hasInitializedExpansion, setHasInitializedExpansion] = useState(false);
  const [isRootExpanded, setIsRootExpanded] = useState(true);

  useEffect(() => {
    if (!hasInitializedExpansion && assets.length > 0) {
      setExpandedAssetIds(new Set(assets.map((asset) => asset.id)));
      setHasInitializedExpansion(true);
    }
  }, [assets, hasInitializedExpansion]);

  const childrenByAsset = new Map<string, Asset[]>();
  const roots: Asset[] = [];
  for (const asset of assets) {
    if (asset.parent_id === undefined || asset.parent_id === null) {
      roots.push(asset);
      continue;
    }
    const children = childrenByAsset.get(asset.parent_id) ?? [];
    children.push(asset);
    childrenByAsset.set(asset.parent_id, children);
  }
  const devicesByAsset = new Map<string, Device[]>();
  const unassigned: Device[] = [];
  for (const device of devices) {
    if (device.asset_id === undefined || device.asset_id === null) {
      unassigned.push(device);
      continue;
    }
    const assigned = devicesByAsset.get(device.asset_id) ?? [];
    assigned.push(device);
    devicesByAsset.set(device.asset_id, assigned);
  }

  const renderDevice = (device: Device) => {
    const name = device.name ?? device.id;
    return (
      <li className="tree-device tree-node" key={device.id}>
        <button
          aria-current={selectedDeviceId === device.id ? "true" : undefined}
          aria-label={"Select " + name}
          className="tree-row"
          onClick={() => onSelectDevice(device.id)}
          type="button"
        >
          <span aria-hidden="true" className={device.online ? "status-dot online" : "status-dot"} />
          <span className="tree-label">{name}</span>
        </button>
      </li>
    );
  };

  const renderAsset = (asset: Asset, seen: Set<string>): React.ReactNode => {
    if (seen.has(asset.id)) {
      return null;
    }
    const nextSeen = new Set(seen);
    nextSeen.add(asset.id);
    const children = (childrenByAsset.get(asset.id) ?? []).sort(assetName);
    const assignedDevices = (devicesByAsset.get(asset.id) ?? []).sort(deviceName);
    const hasChildren = children.length > 0 || assignedDevices.length > 0;
    const isExpanded = expandedAssetIds.has(asset.id);
    return (
      <li className="tree-asset tree-node" key={asset.id}>
        <div className="tree-node-row">
          {hasChildren ? (
            <button
              aria-expanded={isExpanded}
              aria-label={(isExpanded ? "Collapse " : "Expand ") + asset.name}
              className="tree-disclosure"
              onClick={() => {
                setExpandedAssetIds((current) => {
                  const next = new Set(current);
                  if (next.has(asset.id)) {
                    next.delete(asset.id);
                  } else {
                    next.add(asset.id);
                  }
                  return next;
                });
              }}
              type="button"
            >
              <span aria-hidden="true">{isExpanded ? "-" : "+"}</span>
            </button>
          ) : (
            <span aria-hidden="true" className="tree-disclosure-spacer" />
          )}
          <button
            aria-current={selectedAssetId === asset.id ? "true" : undefined}
            aria-label={"Select " + asset.name}
            className="tree-row"
            onClick={() => onSelectAsset(asset.id)}
            type="button"
          >
            <span aria-hidden="true" className="asset-mark" />
            <span className="tree-label">{asset.name}</span>
            {hasChildren && <span className="tree-count">{children.length + assignedDevices.length}</span>}
          </button>
        </div>
        {hasChildren && isExpanded && (
          <ul className="tree-children">
            {children.map((child) => renderAsset(child, nextSeen))}
            {assignedDevices.map((device) => renderDevice(device))}
          </ul>
        )}
      </li>
    );
  };

  return (
    <nav aria-label="Resources" className="powermonitor-tree">
      <ul>
        <li className="tree-root">
          <button
            aria-current={isOverview ? "true" : undefined}
            aria-label="Home"
            className="tree-root-toggle tree-home-toggle"
            onClick={onSelectHome}
            type="button"
          >
            <span aria-hidden="true">⌂</span>
            <strong>Home</strong>
            <span aria-hidden="true" />
          </button>
        </li>
        <li className="tree-root">
          <button
            aria-expanded={isRootExpanded}
            aria-label={isRootExpanded ? "Collapse root" : "Expand root"}
            className="tree-root-toggle"
            onClick={() => setIsRootExpanded((expanded) => !expanded)}
            type="button"
          >
            <span aria-hidden="true">{isRootExpanded ? "−" : "+"}</span>
            <strong>Root</strong>
            <span className="tree-count">{assets.length + unassigned.length}</span>
          </button>
          {isRootExpanded && (
            <ul className="tree-children">
              {roots.sort(assetName).map((asset) => renderAsset(asset, new Set()))}
              {unassigned.sort(deviceName).map((device) => renderDevice(device))}
            </ul>
          )}
        </li>
      </ul>
      {assets.length === 0 && devices.length === 0 && (
        <p className="empty-state">No devices or assets are available.</p>
      )}
    </nav>
  );
}

function assetName(left: Asset, right: Asset): number {
  return left.name.localeCompare(right.name);
}

function deviceName(left: Device, right: Device): number {
  return (left.name ?? left.id).localeCompare(right.name ?? right.id);
}
