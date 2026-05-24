import { defineConfig } from "vitepress";

export default defineConfig({
  title: "topic-tree-with-qav",
  description: "Host-led, audience-interactive sessions with topic trees, Q&A voting, and collaborative whiteboards.",
  ignoreDeadLinks: true,
  themeConfig: {
    logo: "/logo.svg",
    nav: [
      { text: "Home", link: "/" },
      { text: "Usage", link: "/usage/host" },
      { text: "Deployment", link: "/deployment/railway" },
      { text: "Architecture", link: "/architecture" },
      { text: "Contributing", link: "/contributing" },
      {
        text: "GitHub",
        link: "https://github.com/clankercode/topic-tree-with-qav",
      },
    ],
    sidebar: [
      {
        text: "Getting Started",
        items: [
          { text: "Home", link: "/" },
          { text: "Host Guide", link: "/usage/host" },
          { text: "Guest Guide", link: "/usage/guest" },
        ],
      },
      {
        text: "Deployment",
        items: [
          { text: "Railway", link: "/deployment/railway" },
          { text: "Self-host", link: "/deployment/self-host" },
        ],
      },
      {
        text: "Reference",
        items: [
          { text: "Architecture", link: "/architecture" },
          { text: "Contributing", link: "/contributing" },
        ],
      },
    ],
    socialLinks: [
      {
        icon: "github",
        link: "https://github.com/clankercode/topic-tree-with-qav",
      },
    ],
  },
});
