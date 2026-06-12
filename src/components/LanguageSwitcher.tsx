import { useTranslation } from "react-i18next";
import { Languages } from "lucide-react";

export function LanguageSwitcher() {
  const { i18n, t } = useTranslation();
  const currentLang = i18n.language;

  const toggleLanguage = () => {
    const newLang = currentLang === "zh" ? "en" : "zh";

    i18n.changeLanguage(newLang).catch((error) => {
      console.error("Failed to change language:", error);
    });

    try {
      localStorage.setItem("language", newLang);
    } catch (error) {
      console.warn("Failed to save language preference:", error);
    }
  };

  return (
    <button
      onClick={toggleLanguage}
      className="flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm transition-all border border-border hover:border-primary/50 hover:bg-primary/5"
      title={t("language.switchTitle")}
    >
      <Languages className="w-4 h-4 text-muted-foreground" />
      <span className="font-medium">
        {currentLang === "zh" ? (
          <>
            <span className="text-primary">{t("language.zh")}</span>
            <span className="text-muted-foreground mx-1">/</span>
            <span className="text-muted-foreground">{t("language.en")}</span>
          </>
        ) : (
          <>
            <span className="text-muted-foreground">{t("language.zh")}</span>
            <span className="text-muted-foreground mx-1">/</span>
            <span className="text-primary">{t("language.en")}</span>
          </>
        )}
      </span>
    </button>
  );
}
