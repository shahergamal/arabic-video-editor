import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, thumbnailOf } from "./api";
import { useT } from "./i18n";
import type { Brand } from "./types";
import { EMPTY_BRAND, IMAGE_EXT } from "./types";

/** Brand colours, font and logo, saved on this computer for every project. */
export function useBrand() {
  const [brand, setBrandState] = useState<Brand>(EMPTY_BRAND);
  useEffect(() => {
    api
      .brand()
      .then((b) => b && setBrandState({ ...EMPTY_BRAND, ...b }))
      .catch(() => {});
  }, []);
  const setBrand = (b: Brand) => {
    setBrandState(b);
    api.setBrand(b).catch(() => {});
  };
  return [brand, setBrand] as const;
}

export function BrandPanel(props: {
  brand: Brand;
  families: string[];
  onChange: (b: Brand) => void;
  onAddLogo: (path: string) => void;
}) {
  const t = useT();
  const { brand, onChange } = props;
  const [color, setColor] = useState("#e8a33d");
  const [logoUrl, setLogoUrl] = useState<string | null>(null);
  useEffect(() => {
    if (brand.logo) thumbnailOf(brand.logo).then(setLogoUrl).catch(() => setLogoUrl(null));
    else setLogoUrl(null);
  }, [brand.logo]);

  return (
    <div className="panel-body">
      <h3>{t("brand")}</h3>
      <p className="hint small">{t("brandHint")}</p>
      <span className="label">{t("brandColors")}</span>
      <div className="swatches">
        {brand.colors.map((c) => (
          <button
            key={c}
            className="swatch"
            style={{ background: c }}
            title={`${c} · ${t("removeLogo")}`}
            aria-label={c}
            onClick={() => onChange({ ...brand, colors: brand.colors.filter((x) => x !== c) })}
          />
        ))}
        <input type="color" value={color} onChange={(e) => setColor(e.target.value)} />
        <button onClick={() => !brand.colors.includes(color) && onChange({ ...brand, colors: [...brand.colors, color] })}>{t("addColor")}</button>
      </div>
      <label>
        {t("brandFont")}
        <select value={brand.font ?? ""} onChange={(e) => onChange({ ...brand, font: e.target.value || null })}>
          <option value="">{t("none")}</option>
          {props.families.map((f) => (
            <option key={f} value={f}>
              {f}
            </option>
          ))}
        </select>
      </label>
      <span className="label">{t("brandLogo")}</span>
      {logoUrl && <img className="logo-thumb" src={logoUrl} alt={t("brandLogo")} />}
      <div className="row">
        <button
          onClick={async () => {
            const p = await openDialog({ title: t("chooseLogo"), filters: [{ name: "Image", extensions: IMAGE_EXT }] });
            if (typeof p === "string") onChange({ ...brand, logo: p });
          }}
        >
          {t("chooseLogo")}
        </button>
        {brand.logo && <button onClick={() => onChange({ ...brand, logo: null })}>{t("removeLogo")}</button>}
      </div>
      {brand.logo && (
        <button className="primary" onClick={() => props.onAddLogo(brand.logo!)}>
          {t("addLogo")}
        </button>
      )}
    </div>
  );
}
