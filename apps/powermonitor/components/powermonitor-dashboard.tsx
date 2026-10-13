"use client";

import { type FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  BffApiError,
  acceptResourceInvitation,
  cancelResourceInvitation,
  claimDevice,
  createAsset,
  getAssetLiveView,
  getAssetTelemetry,
  getDeviceLiveView,
  getDeviceTelemetry,
  listAlerts,
  listAssets,
  listDevices,
  listResourceProfiles,
  listResourceInvitations,
  listUserCapabilities,
  sendDeviceCommandAndWait,
  updateDevice,
  type Alert,
  type Asset,
  type CommandLifecycle,
  type CommandMode,
  type Device,
  type LiveView,
  type ResourceInvitation,
  type ResourceProfile,
  type TelemetryPoint,
  type TimeRange,
} from "../lib/browser-api";
import { powerProfilePresentation } from "../lib/power-profiles";
import { CommandPanel } from "./command-panel";
import { DeviceControlPanel } from "./device-control-panel";
import { LiveTelemetryCharts } from "./live-telemetry-charts";
import { PowerTelemetryTable } from "./power-telemetry-table";
import { PowerMonitorTree } from "./powermonitor-tree";
import { ResourceEditDrawer } from "./resource-edit-drawer";

type PowerMonitorDashboardProps = {
  initialAssetId?: string;
  initialDeviceId?: string;
};

type BarcodeDetectorLike = {
  detect(source: ImageBitmapSource): Promise<Array<{ rawValue?: string }>>;
};

type BarcodeDetectorConstructor = new (options: { formats: string[] }) => BarcodeDetectorLike;
type CreationOrigin = {
  assetId: string | null;
  deviceId: string | null;
  isOverview: boolean;
};

const telemetryRange: TimeRange = "1h";

function parsePairingQr(value: string): { serialNumber: string; code: string } {
  const uri = new URL(value);
  if (uri.protocol !== "iotnano:" || uri.hostname !== "claim") {
    throw new Error("This QR code is not an IoT Nano pairing code.");
  }
  const serialNumber = uri.searchParams.get("serial_number")?.trim() ?? "";
  const code = uri.searchParams.get("code")?.trim() ?? "";
  if (serialNumber.length === 0 || code.length === 0) {
    throw new Error("The pairing QR code is incomplete.");
  }
  return { serialNumber, code };
}

export function PowerMonitorDashboard({
  initialAssetId,
  initialDeviceId,
}: PowerMonitorDashboardProps) {
  const initialOverview = initialAssetId === undefined && initialDeviceId === undefined;
  const [assets, setAssets] = useState<Asset[]>([]);
  const [assetProfileNames, setAssetProfileNames] = useState<Record<string, string>>({});
  const [assetProfiles, setAssetProfiles] = useState<ResourceProfile[]>([]);
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [accountOpen, setAccountOpen] = useState(false);
  const [assetBusy, setAssetBusy] = useState(false);
  const [assetName, setAssetName] = useState("");
  const [assetOpen, setAssetOpen] = useState(false);
  const [assetParentId, setAssetParentId] = useState("");
  const [assetProfileId, setAssetProfileId] = useState("");
  const [command, setCommand] = useState<CommandLifecycle | null>(null);
  const [commandBusy, setCommandBusy] = useState(false);
  const [claimBusy, setClaimBusy] = useState(false);
  const [claimCode, setClaimCode] = useState("");
  const [claimAssetId, setClaimAssetId] = useState<string | null>(null);
  const [claimSerialNumber, setClaimSerialNumber] = useState("");
  const [creationOrigin, setCreationOrigin] = useState<CreationOrigin | null>(null);
  const [claimOpen, setClaimOpen] = useState(false);
  const [capabilities, setCapabilities] = useState<string[]>([]);
  const [devices, setDevices] = useState<Device[]>([]);
  const [deviceProfileNames, setDeviceProfileNames] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [invitationBusyId, setInvitationBusyId] = useState<string | null>(null);
  const [invitations, setInvitations] = useState<ResourceInvitation[]>([]);
  const [invitationsOpen, setInvitationsOpen] = useState(false);
  const [liveView, setLiveView] = useState<LiveView>({ charts: [], profile: null });
  const [liveViewLoading, setLiveViewLoading] = useState(false);
  const [pendingAsset, setPendingAsset] = useState<Asset | null>(null);
  const [pendingDevice, setPendingDevice] = useState<Device | null>(null);
  const [editingResource, setEditingResource] = useState<{
    asset_id?: string | null;
    id: string;
    kind: "asset" | "device";
    name?: string;
    parent_id?: string | null;
    permission?: Device["permission"] | Asset["permission"];
  } | null>(null);
  const [selectedAssetId, setSelectedAssetId] = useState<string | null>(initialAssetId ?? null);
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(initialDeviceId ?? null);
  const [isOverview, setIsOverview] = useState(initialOverview);
  const [telemetry, setTelemetry] = useState<TelemetryPoint[]>([]);
  const [telemetryLoading, setTelemetryLoading] = useState(false);
  const selectedAssetIdRef = useRef<string | null>(initialAssetId ?? null);
  const selectedDeviceIdRef = useRef<string | null>(initialDeviceId ?? null);
  const isOverviewRef = useRef(initialOverview);
  const selectionGenerationRef = useRef(0);
  const telemetryRequestIdRef = useRef(0);
  const liveViewRequestIdRef = useRef(0);
  const workspaceRequestIdRef = useRef(0);

  const selectedDevice = useMemo(
    () => devices.find((device) => device.id === selectedDeviceId) ?? null,
    [devices, selectedDeviceId],
  );
  const selectedAsset = useMemo(
    () => assets.find((asset) => asset.id === selectedAssetId) ?? null,
    [assets, selectedAssetId],
  );
  const selectedProfileName = selectedDevice?.device_profile_id
    ? deviceProfileNames[selectedDevice.device_profile_id]
    : selectedAsset?.asset_profile_id
      ? assetProfileNames[selectedAsset.asset_profile_id]
      : undefined;
  const onlineCount = devices.filter((device) => device.online).length;
  const treeAssets = pendingAsset === null ? assets : [...assets, pendingAsset];
  const treeDevices = pendingDevice === null ? devices : [...devices, pendingDevice];
  const selectedResource = selectedAsset !== null
    ? {
      id: selectedAsset.id,
      kind: "asset" as const,
      name: selectedAsset.name,
      parent_id: selectedAsset.parent_id,
      permission: selectedAsset.permission,
      asset_profile_id: selectedAsset.asset_profile_id,
      profile_name: selectedProfileName,
    }
    : selectedDevice !== null
      ? {
        asset_id: selectedDevice.asset_id,
        id: selectedDevice.id,
        kind: "device" as const,
        name: selectedDevice.name,
        permission: selectedDevice.permission,
        device_profile_id: selectedDevice.device_profile_id,
        profile_name: selectedProfileName,
      }
      : null;
  const canManageSelectedResource = selectedResource?.permission === "manager"
    || selectedResource?.permission === "owner";
  const canCreateAssets = capabilities.includes("create_assets");
  const canClaimDevices = capabilities.includes("claim_devices");
  const resourcePath = useMemo(
    () => getResourcePath(assets, selectedAsset, selectedDevice),
    [assets, selectedAsset, selectedDevice],
  );
  const profilePresentation = powerProfilePresentation(
    selectedDevice !== null ? "device" : "asset",
    selectedProfileName,
  );
  const effectiveLiveView = liveView.profile !== null || profilePresentation === null
    ? liveView
    : {
      charts: profilePresentation.charts,
      profile: {
        id: selectedDevice?.device_profile_id ?? selectedAsset?.asset_profile_id ?? profilePresentation.label,
        name: profilePresentation.label,
      },
    };

  const refresh = useCallback(async (): Promise<boolean> => {
    const requestId = workspaceRequestIdRef.current + 1;
    const selectionGeneration = selectionGenerationRef.current;
    workspaceRequestIdRef.current = requestId;
    setError(null);
    try {
      const [[nextDevices, nextAssets, nextAlerts], [nextDeviceProfiles, nextAssetProfiles], invitationRead, nextCapabilities] = await Promise.all([
        Promise.all([listDevices(), listAssets(), listAlerts()]),
        Promise.all([listResourceProfiles("device").catch(() => []), listResourceProfiles("asset").catch(() => [])]),
        readResourceInvitations(),
        readUserCapabilities(),
      ]);
      if (
        workspaceRequestIdRef.current !== requestId
        || selectionGenerationRef.current !== selectionGeneration
      ) {
        return false;
      }
      setDevices(nextDevices);
      setAssets(nextAssets);
      setDeviceProfileNames(Object.fromEntries(nextDeviceProfiles.map((profile) => [profile.id, profile.name])));
      setAssetProfileNames(Object.fromEntries(nextAssetProfiles.map((profile) => [profile.id, profile.name])));
      setAssetProfiles(nextAssetProfiles);
      setAlerts(nextAlerts);
      setCapabilities(nextCapabilities);
      setInvitations(invitationRead.items);
      if (invitationRead.warning !== null) {
        setError(invitationRead.warning);
      }
      const requestedAssetId = selectedAssetIdRef.current;
      const requestedDeviceId = selectedDeviceIdRef.current;
      if (requestedAssetId === "pending-asset" || requestedDeviceId === "pending-device") {
        setSelectedAssetId(requestedAssetId);
        setSelectedDeviceId(requestedDeviceId);
        return true;
      }
      const nextAssetId = !isOverviewRef.current && requestedAssetId !== null && nextAssets.some((asset) => asset.id === requestedAssetId)
        ? requestedAssetId
        : null;
      const nextDeviceId = isOverviewRef.current || nextAssetId !== null
        ? null
        : requestedDeviceId !== null && nextDevices.some((device) => device.id === requestedDeviceId)
          ? requestedDeviceId
          : nextDevices[0]?.id ?? null;
      selectedAssetIdRef.current = nextAssetId;
      selectedDeviceIdRef.current = nextDeviceId;
      setSelectedAssetId(nextAssetId);
      setSelectedDeviceId(nextDeviceId);
      return true;
    } catch (reason) {
      if (
        workspaceRequestIdRef.current === requestId
        && selectionGenerationRef.current === selectionGeneration
      ) {
        setError(errorMessage(reason));
      }
      return false;
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const refreshTelemetry = useCallback(async (): Promise<void> => {
    const assetId = selectedAssetIdRef.current;
    const deviceId = selectedDeviceIdRef.current;
    if (assetId === "pending-asset" || deviceId === "pending-device") {
      telemetryRequestIdRef.current += 1;
      setTelemetry([]);
      return;
    }
    if (assetId === null && deviceId === null) {
      telemetryRequestIdRef.current += 1;
      setTelemetry([]);
      return;
    }
    const requestId = telemetryRequestIdRef.current + 1;
    telemetryRequestIdRef.current = requestId;
    setTelemetryLoading(true);
    try {
      const points = assetId !== null
        ? await getAssetTelemetry(assetId, telemetryRange)
        : await getDeviceTelemetry(deviceId as string, telemetryRange);
      if (telemetryRequestIdRef.current === requestId) {
        setTelemetry(points);
      }
    } catch (reason) {
      if (telemetryRequestIdRef.current === requestId) {
        setError(errorMessage(reason));
      }
    } finally {
      if (telemetryRequestIdRef.current === requestId) {
        setTelemetryLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    void refreshTelemetry();
  }, [refreshTelemetry, selectedAssetId, selectedDeviceId]);

  const refreshLiveView = useCallback(async (): Promise<void> => {
    const assetId = selectedAssetIdRef.current;
    const deviceId = selectedDeviceIdRef.current;
    const target = assetId !== null
      ? { id: assetId, type: "asset" as const }
      : deviceId !== null
        ? { id: deviceId, type: "device" as const }
        : null;
    if (target?.id === "pending-asset" || target?.id === "pending-device") {
      setLiveView({ charts: [], profile: null });
      setLiveViewLoading(false);
      return;
    }
    const requestId = liveViewRequestIdRef.current + 1;
    liveViewRequestIdRef.current = requestId;
    if (target === null) {
      setLiveView({ charts: [], profile: null });
      setLiveViewLoading(false);
      return;
    }
    setLiveViewLoading(true);
    setLiveView({ charts: [], profile: null });
    try {
      const view = target.type === "asset"
        ? await getAssetLiveView(target.id)
        : await getDeviceLiveView(target.id);
      if (liveViewRequestIdRef.current === requestId) {
        setLiveView(view);
      }
    } catch (reason) {
      if (liveViewRequestIdRef.current === requestId) {
        setError(errorMessage(reason));
      }
    } finally {
      if (liveViewRequestIdRef.current === requestId) {
        setLiveViewLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    void refreshLiveView();
  }, [refreshLiveView, selectedAssetId, selectedDeviceId]);

  useEffect(() => {
    const refreshVisibleTelemetry = () => {
      if (!document.hidden) {
        void refreshTelemetry();
      }
    };
    const interval = window.setInterval(refreshVisibleTelemetry, 5_000);
    document.addEventListener("visibilitychange", refreshVisibleTelemetry);
    return () => {
      window.clearInterval(interval);
      document.removeEventListener("visibilitychange", refreshVisibleTelemetry);
    };
  }, [refreshTelemetry]);

  const refreshWorkspaceAndTelemetry = useCallback(async (): Promise<void> => {
    if (await refresh()) {
      await refreshTelemetry();
    }
  }, [refresh, refreshTelemetry]);

  const selectPendingResource = (kind: "asset" | "device") => {
    setCreationOrigin({
      assetId: selectedAssetIdRef.current,
      deviceId: selectedDeviceIdRef.current,
      isOverview: isOverviewRef.current,
    });
    selectionGenerationRef.current += 1;
    isOverviewRef.current = false;
    setIsOverview(false);
    selectedAssetIdRef.current = kind === "asset" ? "pending-asset" : null;
    selectedDeviceIdRef.current = kind === "device" ? "pending-device" : null;
    setSelectedAssetId(selectedAssetIdRef.current);
    setSelectedDeviceId(selectedDeviceIdRef.current);
  };

  const restoreCreationOrigin = () => {
    if (creationOrigin === null) return;
    selectionGenerationRef.current += 1;
    isOverviewRef.current = creationOrigin.isOverview;
    setIsOverview(creationOrigin.isOverview);
    selectedAssetIdRef.current = creationOrigin.assetId;
    selectedDeviceIdRef.current = creationOrigin.deviceId;
    setSelectedAssetId(creationOrigin.assetId);
    setSelectedDeviceId(creationOrigin.deviceId);
    setCreationOrigin(null);
  };

  const submitAsset = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = assetName.trim();
    if (name.length === 0) {
      return;
    }
    setAssetBusy(true);
    setError(null);
    try {
      const asset = await createAsset({
        asset_profile_id: assetProfileId === "" ? null : assetProfileId,
        name,
        parent_asset_id: assetParentId === "" ? null : assetParentId,
      });
      setAssetName("");
      setAssetParentId("");
      setAssetProfileId("");
      setAssetOpen(false);
      isOverviewRef.current = false;
      setIsOverview(false);
      selectedAssetIdRef.current = asset.id;
      selectedDeviceIdRef.current = null;
      setSelectedAssetId(asset.id);
      setSelectedDeviceId(null);
      await refreshWorkspaceAndTelemetry();
      setPendingAsset(null);
      setCreationOrigin(null);
    } catch (reason) {
      setPendingAsset(null);
      restoreCreationOrigin();
      setError(errorMessage(reason));
    } finally {
      setAssetBusy(false);
    }
  };

  const submitDeviceClaim = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const serialNumber = claimSerialNumber.trim();
    const code = claimCode.trim();
    if (serialNumber.length === 0 || code.length === 0) {
      return;
    }
    setClaimBusy(true);
    setError(null);
    try {
      let device = await claimDevice(serialNumber, code);
      if (claimAssetId !== null) {
        device = await updateDevice(device.id, { asset_id: claimAssetId });
      }
      setClaimCode("");
      setClaimSerialNumber("");
      setClaimAssetId(null);
      setClaimOpen(false);
      isOverviewRef.current = false;
      setIsOverview(false);
      selectedAssetIdRef.current = null;
      selectedDeviceIdRef.current = device.id;
      setSelectedAssetId(null);
      setSelectedDeviceId(device.id);
      await refreshWorkspaceAndTelemetry();
      setPendingDevice(null);
      setCreationOrigin(null);
    } catch (reason) {
      setPendingDevice(null);
      restoreCreationOrigin();
      setError(errorMessage(reason));
    } finally {
      setClaimBusy(false);
    }
  };

  const scanClaimQr = async (file: File) => {
    const BarcodeDetector = (window as Window & { BarcodeDetector?: BarcodeDetectorConstructor }).BarcodeDetector;
    if (!BarcodeDetector) {
      setError("QR scanning is unavailable in this browser. Enter the serial number and pairing code manually.");
      return;
    }
    setClaimBusy(true);
    setError(null);
    try {
      const bitmap = await createImageBitmap(file);
      try {
        const code = (await new BarcodeDetector({ formats: ["qr_code"] }).detect(bitmap))[0]?.rawValue;
        if (!code) throw new Error("No QR code was found in the selected image.");
        const pairing = parsePairingQr(code);
        setClaimSerialNumber(pairing.serialNumber);
        setClaimCode(pairing.code);
      } finally {
        bitmap.close();
      }
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setClaimBusy(false);
    }
  };

  const sendCommand = async (
    method: string,
    params: Record<string, unknown>,
    mode: CommandMode = "one_way",
  ) => {
    if (selectedDevice === null) {
      return;
    }
    setCommandBusy(true);
    setError(null);
    try {
      const lifecycle = await sendDeviceCommandAndWait(
        selectedDevice.id,
        method,
        params,
        mode,
        { onProgress: setCommand },
      );
      setCommand(lifecycle);
      await refreshWorkspaceAndTelemetry();
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setCommandBusy(false);
    }
  };

  const respondToInvitation = async (invitationId: string, action: "accept" | "cancel") => {
    setInvitationBusyId(invitationId);
    setError(null);
    try {
      if (action === "accept") {
        await acceptResourceInvitation(invitationId);
      } else {
        await cancelResourceInvitation(invitationId);
      }
      setInvitations((current) => current.filter((invitation) => invitation.id !== invitationId));
      if (action === "accept") {
        await refreshWorkspaceAndTelemetry();
      } else {
        await refresh();
      }
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setInvitationBusyId(null);
    }
  };

  const pendingTitle = selectedAssetId === "pending-asset"
    ? "Configuring asset"
    : selectedDeviceId === "pending-device"
      ? "Configuring device"
      : null;
  const title = isOverview ? "Overview" : pendingTitle ?? selectedDevice?.name ?? selectedDevice?.id ?? selectedAsset?.name ?? "Fleet overview";
  return (
    <main className="powermonitor-shell">
      <header className="app-menu">
        <div className="brand-lockup">
          <span className="brand-mark" aria-hidden="true">P</span>
          <strong>Power Monitor</strong>
        </div>
        <nav aria-label="Power Monitor actions" className="app-menu-actions">
          <button
            aria-controls="resource-invitations"
            aria-expanded={invitationsOpen}
            aria-label="Invitations"
            className="app-menu-action"
            onClick={() => setInvitationsOpen((open) => !open)}
            title="Invitations"
            type="button"
          >
            <svg aria-hidden="true" viewBox="0 0 24 24"><path d="M3 6h18v12H3zM3 7l9 6 9-6" /></svg>
            {invitations.length > 0 && <span className="app-menu-badge">{invitations.length}</span>}
          </button>
          <div className="app-account-menu">
            <button
              aria-expanded={accountOpen}
              aria-label="Profile"
              className="app-menu-action"
              onClick={() => setAccountOpen((open) => !open)}
              title="Profile"
              type="button"
            >
              <svg aria-hidden="true" viewBox="0 0 24 24"><circle cx="12" cy="8" r="3.5" /><path d="M5 20c.8-3.4 3.1-5 7-5s6.2 1.6 7 5" /></svg>
            </button>
            {accountOpen && (
              <div aria-label="Profile menu" className="account-popover" role="menu">
                {selectedResource !== null && canManageSelectedResource && (
                  <button
                    onClick={() => {
                      setEditingResource(selectedResource);
                      setAccountOpen(false);
                    }}
                    role="menuitem"
                    type="button"
                  >
                    Edit selected resource
                  </button>
                )}
                <div className="account-popover-session-actions">
                  <form action="/api/v1/auth/logout" method="post">
                    <button role="menuitem" type="submit">Sign out</button>
                  </form>
                  <a href="/api/v1/docs" rel="noreferrer" role="menuitem" target="_blank">
                    Open API docs
                  </a>
                </div>
              </div>
            )}
          </div>
        </nav>
      </header>
      <aside className="explorer">
        <div aria-label="Create resources" className="explorer-create-actions">
          {canCreateAssets && (
            <button
              aria-expanded={assetOpen}
              aria-label="Add asset"
              className="explorer-create-button"
              disabled={claimOpen || assetBusy}
              onClick={() => {
                if (assetOpen) {
                  setAssetOpen(false);
                  setPendingAsset(null);
                  restoreCreationOrigin();
                  return;
                }
                const parentAssetId = selectedAsset?.id ?? selectedDevice?.asset_id ?? null;
                setAssetParentId(parentAssetId ?? "");
                setPendingAsset({
                  id: "pending-asset",
                  name: "Configuring asset",
                  parent_id: parentAssetId,
                });
                selectPendingResource("asset");
                setAssetOpen(true);
              }}
              title="Add asset"
              type="button"
            >
              Add asset
            </button>
          )}
          {canClaimDevices && (
            <button
              aria-expanded={claimOpen}
              aria-label="Add device"
              className="explorer-create-button"
              disabled={assetOpen || claimBusy}
              onClick={() => {
                if (claimOpen) {
                  setClaimOpen(false);
                  setClaimAssetId(null);
                  setPendingDevice(null);
                  restoreCreationOrigin();
                  return;
                }
                const targetAssetId = selectedAsset?.id ?? selectedDevice?.asset_id ?? null;
                setClaimAssetId(targetAssetId);
                setPendingDevice(targetAssetId === null
                  ? null
                  : {
                    asset_id: targetAssetId,
                    id: "pending-device",
                    name: "Configuring device",
                    online: false,
                  });
                selectPendingResource("device");
                setClaimOpen(true);
              }}
              title="Add device"
              type="button"
            >
              Add device
            </button>
          )}
        </div>
        <PowerMonitorTree
          assets={treeAssets}
          devices={treeDevices}
          isOverview={isOverview}
          onSelectAsset={(assetId) => {
            selectionGenerationRef.current += 1;
            setEditingResource(null);
            isOverviewRef.current = false;
            setIsOverview(false);
            selectedAssetIdRef.current = assetId;
            selectedDeviceIdRef.current = null;
            setSelectedAssetId(assetId);
            setSelectedDeviceId(null);
          }}
          onSelectDevice={(deviceId) => {
            selectionGenerationRef.current += 1;
            setEditingResource(null);
            isOverviewRef.current = false;
            setIsOverview(false);
            selectedAssetIdRef.current = null;
            selectedDeviceIdRef.current = deviceId;
            setSelectedDeviceId(deviceId);
            setSelectedAssetId(null);
          }}
          onSelectHome={() => {
            selectionGenerationRef.current += 1;
            setEditingResource(null);
            isOverviewRef.current = true;
            setIsOverview(true);
            selectedAssetIdRef.current = null;
            selectedDeviceIdRef.current = null;
            setSelectedAssetId(null);
            setSelectedDeviceId(null);
          }}
          selectedAssetId={selectedAssetId}
          selectedDeviceId={selectedDeviceId}
        />
      </aside>

      <section className="workspace">
        <header className="workspace-header">
          <div>
            <nav aria-label="Resource path" className="resource-path">
              <ol>
                {resourcePath.map((entry, index) => <li key={entry + index}>{entry}</li>)}
              </ol>
            </nav>
            <h1>{title}</h1>
            <div className="resource-meta">
              {selectedDevice !== null && (
                <span className={selectedDevice.online ? "resource-status online" : "resource-status"}>
                  {selectedDevice.online ? "Online" : "Offline"}
                </span>
              )}
              {selectedProfileName !== undefined && <span>{selectedProfileName}</span>}
              <span>{selectedDevice?.id ?? selectedAsset?.id ?? "All accessible resources"}</span>
            </div>
          </div>
        </header>

        {invitationsOpen && (
          <section aria-label="Pending invitations" className="invitation-panel" id="resource-invitations">
            <header className="section-heading">
              <div>
                <span className="eyebrow">Resource access</span>
                <h2>Invitations</h2>
              </div>
            </header>
            {invitations.length === 0 ? (
              <p className="empty-state">No pending invitations.</p>
            ) : (
              <ul className="invitation-list">
                {invitations.map((invitation) => (
                  <li key={invitation.id}>
                    <div>
                      <strong>{invitation.resource_name}</strong>
                      <span>{invitation.sender_username} shared {invitation.permission} access to this {invitation.resource_kind}.</span>
                    </div>
                    <div className="invitation-actions">
                      <button
                        aria-label={"Accept invitation for " + invitation.resource_name}
                        disabled={invitationBusyId === invitation.id}
                        onClick={() => void respondToInvitation(invitation.id, "accept")}
                        type="button"
                      >
                        Accept
                      </button>
                      <button
                        aria-label={"Decline invitation for " + invitation.resource_name}
                        disabled={invitationBusyId === invitation.id}
                        onClick={() => void respondToInvitation(invitation.id, "cancel")}
                        type="button"
                      >
                        Decline
                      </button>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </section>
        )}

        {claimOpen && (
          <section aria-label="Add device" className="claim-device-panel">
            <header className="section-heading"><h2>Add device</h2></header>
            <form onSubmit={(event) => void submitDeviceClaim(event)}>
              <label>
                <span>Serial number</span>
                <input autoComplete="off" disabled={claimBusy} onChange={(event) => setClaimSerialNumber(event.target.value)} value={claimSerialNumber} />
              </label>
              <label>
                <span>Pairing code</span>
                <input autoComplete="one-time-code" disabled={claimBusy} onChange={(event) => setClaimCode(event.target.value)} value={claimCode} />
              </label>
              <label>
                <span>Scan pairing QR</span>
                <input
                  accept="image/*"
                  capture="environment"
                  disabled={claimBusy}
                  onChange={(event) => {
                    const file = event.currentTarget.files?.[0];
                    event.currentTarget.value = "";
                    if (file) void scanClaimQr(file);
                  }}
                  type="file"
                />
              </label>
              <div className="invitation-actions">
                <button disabled={claimBusy || claimSerialNumber.trim().length === 0 || claimCode.trim().length === 0} type="submit">Add device</button>
                <button
                  disabled={claimBusy}
                  onClick={() => {
                    setClaimOpen(false);
                    setClaimAssetId(null);
                    setPendingDevice(null);
                    restoreCreationOrigin();
                  }}
                  type="button"
                >
                  Cancel
                </button>
              </div>
            </form>
          </section>
        )}

        {assetOpen && (
          <section aria-label="Add asset" className="claim-device-panel">
            <header className="section-heading"><h2>Add asset</h2></header>
            <form onSubmit={(event) => void submitAsset(event)}>
              <label>
                <span>Asset name</span>
                <input autoComplete="off" disabled={assetBusy} onChange={(event) => setAssetName(event.target.value)} required value={assetName} />
              </label>
              <label>
                <span>Parent asset</span>
                <select aria-label="Parent asset" disabled={assetBusy} onChange={(event) => setAssetParentId(event.target.value)} value={assetParentId}>
                  <option value="">No parent</option>
                  {assets.map((asset) => <option key={asset.id} value={asset.id}>{asset.name}</option>)}
                </select>
              </label>
              <label>
                <span>Asset profile</span>
                <select aria-label="Asset profile" disabled={assetBusy} onChange={(event) => setAssetProfileId(event.target.value)} value={assetProfileId}>
                  <option value="">No profile</option>
                  {assetProfiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.name}</option>)}
                </select>
              </label>
              <div className="invitation-actions">
                <button disabled={assetBusy || assetName.trim().length === 0} type="submit">{assetBusy ? "Creating" : "Create asset"}</button>
                <button
                  disabled={assetBusy}
                  onClick={() => {
                    setAssetOpen(false);
                    setPendingAsset(null);
                    restoreCreationOrigin();
                  }}
                  type="button"
                >
                  Cancel
                </button>
              </div>
            </form>
          </section>
        )}

        {error !== null && (
          <div className="error-banner" role="alert">
            <span>{error}</span>
            {(error === "Sign in is required." || error === invitationScopeMessage) && (
              <a href="/api/v1/auth/login">Sign in again</a>
            )}
          </div>
        )}

        {isOverview && (
          <section aria-label="Fleet summary" className="reading-strip">
            <div><span>Devices</span><strong>{devices.length}</strong></div>
            <div><span>Online</span><strong>{onlineCount}</strong></div>
            <div><span>Assets</span><strong>{assets.length}</strong></div>
            <div><span>Alerts</span><strong>{alerts.length}</strong></div>
          </section>
        )}

        {!isOverview && (
        <section aria-label="Telemetry" className="telemetry-section">
          <header className="section-heading">
            <div>
              <span className="eyebrow">{selectedAsset !== null ? "Selected asset" : "Selected device"}</span>
              <h2>Telemetry</h2>
            </div>
            <span>{telemetryLoading ? "Updating" : telemetry.length + " samples"}</span>
          </header>
          {selectedDevice === null && selectedAsset === null ? (
            <>
              <p className="empty-state">Select a device to inspect telemetry and send commands.</p>
              <CommandPanel busy={false} disabled onSubmit={sendCommand} />
            </>
          ) : (
            <>
              {liveViewLoading ? (
                <p className="empty-state">Loading configured live view...</p>
              ) : effectiveLiveView.profile === null ? (
                <PowerTelemetryTable points={telemetry} />
              ) : effectiveLiveView.charts.length === 0 ? (
                <p className="empty-state">This profile has no live widgets configured.</p>
              ) : (
                <LiveTelemetryCharts
                  charts={effectiveLiveView.charts}
                  isAsset={selectedAsset !== null}
                  points={telemetry}
                  range={telemetryRange}
                />
              )}
              {selectedDevice !== null && (
                <>
                  <DeviceControlPanel
                    brightnessPct={selectedDevice.brightness_pct}
                    busy={commandBusy}
                    canControl={selectedDevice.permission !== "viewer"}
                    capabilities={selectedDevice.capabilities}
                    commandState={command?.state ?? null}
                    onCommand={(method, params, mode) => void sendCommand(method, params, mode)}
                    switchState={selectedDevice.switch_state}
                  />
                  <CommandPanel
                    busy={commandBusy}
                    onSubmit={sendCommand}
                    response={command?.response}
                    state={command?.state}
                  />
                </>
              )}
            </>
          )}
        </section>
        )}
      </section>
      {editingResource !== null && (
        <ResourceEditDrawer
          assets={assets}
          onClose={() => setEditingResource(null)}
          onSaved={async () => {
            await refreshWorkspaceAndTelemetry();
            await refreshLiveView();
          }}
          resource={editingResource}
        />
      )}
    </main>
  );
}

function errorMessage(reason: unknown): string {
  if (reason instanceof BffApiError && reason.status === 401) {
    return "Sign in is required.";
  }
  return reason instanceof Error ? reason.message : "PowerMonitor request failed.";
}

const invitationScopeMessage = "Sign in again to use resource invitations.";

type InvitationRead = {
  items: ResourceInvitation[];
  warning: string | null;
};

async function readResourceInvitations(): Promise<InvitationRead> {
  try {
    return { items: await listResourceInvitations(), warning: null };
  } catch (reason) {
    if (reason instanceof BffApiError && reason.status === 403) {
      return { items: [], warning: invitationScopeMessage };
    }
    throw reason;
  }
}

async function readUserCapabilities(): Promise<string[]> {
  try {
    return await listUserCapabilities();
  } catch {
    return [];
  }
}

function getResourcePath(
  assets: Asset[],
  selectedAsset: Asset | null,
  selectedDevice: Device | null,
): string[] {
  const assetsById = new Map(assets.map((asset) => [asset.id, asset]));
  const path: string[] = [];
  const visited = new Set<string>();
  let assetId = selectedAsset?.id ?? selectedDevice?.asset_id ?? null;

  while (assetId !== null && !visited.has(assetId)) {
    visited.add(assetId);
    const asset = assetsById.get(assetId);
    if (asset === undefined) {
      break;
    }
    path.unshift(asset.name);
    assetId = asset.parent_id ?? null;
  }

  if (selectedDevice !== null) {
    path.push(selectedDevice.name ?? selectedDevice.id);
  }

  return path.length > 0 ? path : ["All resources"];
}
