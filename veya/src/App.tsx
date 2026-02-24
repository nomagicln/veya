import { useState } from "react";
import { useTranslation } from "react-i18next";
import SettingsPage from "./components/SettingsPage";
import ApiConfigPage from "./components/ApiConfigPage";
import LearningPage from "./components/LearningPage";
import "./components/Pages.css";
import "./App.css";

type Page = "settings" | "apiConfig" | "learning";

function App() {
  const { t } = useTranslation();
  const [page, setPage] = useState<Page>("settings");

  return (
    <main>
      {page !== "apiConfig" && (
        <nav className="app-nav" aria-label="Main navigation">
          <button
            className={page === "settings" ? "active" : ""}
            onClick={() => setPage("settings")}
          >
            ⚙️ {t("nav.settings")}
          </button>
          <button
            className={page === "learning" ? "active" : ""}
            onClick={() => setPage("learning")}
          >
            📚 {t("nav.learning")}
          </button>
        </nav>
      )}

      {page === "settings" && (
        <SettingsPage onNavigateApiConfig={() => setPage("apiConfig")} />
      )}
      {page === "apiConfig" && (
        <ApiConfigPage onBack={() => setPage("settings")} />
      )}
      {page === "learning" && <LearningPage />}
    </main>
  );
}

export default App;
