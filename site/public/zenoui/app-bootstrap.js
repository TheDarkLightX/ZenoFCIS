import {mount} from './app.js';
const root=document.querySelector('#app');
try {const response=await fetch(new URL('../app.json',import.meta.url));if(!response.ok)throw Error('UiDescriptionUnavailable');await mount(root,await response.json());}
catch {root.textContent='The application could not start. Reload to try again.';root.dataset.phase='error';}
