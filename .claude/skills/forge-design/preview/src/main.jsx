import { render } from 'solid-js/web';
import '@forge/colors_and_type.css';
import '@forge/console.css';
import '@forge/chat.css';
import Gallery from './Gallery';

document.body.style.margin = '0';
render(() => <Gallery />, document.getElementById('root'));
