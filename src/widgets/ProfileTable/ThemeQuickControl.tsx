import { useRef, useState } from "react";
import { IconButton, Menu, useTheme, type Theme } from "@proxyshard/shardx-ui-kit";
import { ComputerIcon, MoonIcon, SunIcon } from "../../shared/icons";
import { useT } from "../../shared/i18n";
import { dropPref } from "../../pages/settings/useSettingsDraft";

/** The same stored theme as Settings and the palette, reachable from the list. */
export function ThemeQuickControl() {
  const t = useT();
  const { theme, setTheme } = useTheme();
  const [open, setOpen] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const choices = [
    { value: "light" as Theme, label: t("settings.themeLight"), Icon: SunIcon },
    { value: "dark" as Theme, label: t("settings.themeDark"), Icon: MoonIcon },
    { value: "system" as Theme, label: t("settings.themeSystem"), Icon: ComputerIcon },
  ];
  const current = choices.find((choice) => choice.value === theme) ?? choices[2];
  const Icon = current.Icon;
  const choose = (value: Theme) => {
    // A quick choice commits immediately, as the palette does; an old Settings
    // preview must not replace it when that draft is opened again.
    setTheme(value);
    dropPref("theme");
  };
  return <>
    <IconButton
      ref={trigger}
      data-theme-quick
      mode="stroke"
      size="xsmall"
      label={`${t("settings.themeTitle")}: ${current.label}`}
      icon={<Icon className="size-3.5" />}
      aria-haspopup="menu"
      aria-expanded={open}
      onClick={() => setOpen((value) => !value)}
    />
    <Menu
      open={open}
      onOpenChange={setOpen}
      anchor={trigger}
      align="end"
      aria-label={t("settings.themeTitle")}
      items={choices.map(({ value, label }) => ({
        type: "radio", label, checked: theme === value, onSelect: () => choose(value),
      }))}
    />
  </>;
}
