import { useCallback } from "react";
import { tnbProxyLocations, type TnbLocation, type TnbLocationsRequest } from "../../../entities/proxy";
import { TnbLocationPicker, type TnbLocationState } from "./TnbLocationPicker";
import { useTnbLocations } from "./useTnbLocations";

export type TnbGeo = { country: TnbLocation | null; region: TnbLocation | null; city: TnbLocation | null };

function useLocations(request: TnbLocationsRequest) {
  const { server, route, level, country, state } = request;
  const key = JSON.stringify([server.trim(), route, level, country ?? "", state ?? ""]);
  const fetch = useCallback(async (): Promise<TnbLocationState> => {
    const result = await tnbProxyLocations({ server: server.trim(), route, level, country, state });
    return { status: result.supported ? "ready" : "unavailable", items: result.items.filter((entry) => entry.code && entry.code !== "0") };
  }, [server, route, level, country, state]);
  return { ...useTnbLocations(key, fetch), key };
}

export function TnbGeoFields({ server, route, value, disabled, onChange }: {
  server: string;
  route: TnbLocationsRequest["route"];
  value: TnbGeo;
  disabled: boolean;
  onChange: (geo: TnbGeo) => void;
}) {
  const country = useLocations({ server, route, level: "country" });
  const region = useLocations({ server, route, level: "state", country: value.country?.code });
  const city = useLocations({ server, route, level: "city", country: value.country?.code, state: value.region?.code });
  return (
    <div className="grid grid-cols-3 gap-3" data-tnb-geo>
      <TnbLocationPicker key={country.key} level="country" value={value.country} state={country.state} disabled={disabled} onLoad={country.load} onChange={(next) => onChange({ country: next, region: null, city: null })} />
      <TnbLocationPicker key={region.key} level="region" value={value.region} state={region.state} disabled={disabled || !value.country} onLoad={region.load} onChange={(next) => onChange({ ...value, region: next, city: null })} />
      <TnbLocationPicker key={city.key} level="city" value={value.city} state={city.state} disabled={disabled || !value.region} onLoad={city.load} onChange={(next) => onChange({ ...value, city: next })} />
    </div>
  );
}
