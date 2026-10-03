import '@fontsource-variable/inter';
import '@fontsource-variable/sora';
import '@fontsource-variable/geist-mono';
import { StrictMode } from 'react';
import { createRoot, hydrateRoot } from 'react-dom/client';
import './index.css';
import { Site } from './App';
import { initialLocation } from './lib/router';

const url = initialLocation();
const root = document.getElementById('root')!;
const app = (
  <StrictMode>
    <Site url={url} />
  </StrictMode>
);

if (root.dataset.path === window.location.pathname) hydrateRoot(root, app);
else createRoot(root).render(app);
