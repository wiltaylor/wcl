/* Token CSS must load before component styles so var(--token) resolves. */
import '@forge/tokens/fonts.css';
import '@forge/tokens/tokens.css';
import '@forge/tokens/base.css';
import '@forge/ui/styles.css';
import '@forge/code/styles.css';
import './app.css';

import { render } from 'solid-js/web';
import App from './App';

render(() => <App />, document.getElementById('root'));
