// Плагин получает запрос аргументом и печатает JSON-массив результатов.
// Wisp проверит ответ: максимум 100 пунктов, только http(s)-ссылки, ограничения длины.
const query = (process.argv[2] ?? "").trim();
const q = encodeURIComponent(query);

const items = query
  ? [
      { title: `Яндекс: ${query}`, subtitle: "ya.ru", url: `https://ya.ru/search/?text=${q}` },
      { title: `Google: ${query}`, subtitle: "google.com", url: `https://www.google.com/search?q=${q}` },
      { title: `GitHub: ${query}`, subtitle: "github.com", url: `https://github.com/search?q=${q}` },
      { title: `docs.rs: ${query}`, subtitle: "документация Rust-крейтов", url: `https://docs.rs/releases/search?query=${q}` },
      { title: "Скопировать запрос", subtitle: query, copy: query },
    ]
  : [{ title: "Введите запрос после «ws»", copy: "" }];

process.stdout.write(JSON.stringify(items));
