const fullScreen = ():void => {
  const mainContainer = document.getElementById('main-container');
  const panels = document.querySelectorAll(".panel");
  const allButtons: HTMLSpanElement[] = [];//temparary fix of 3 click bug issue

  panels.forEach(panel =>{
    const panelHeader = panel.firstElementChild as HTMLElement;
    if (!panelHeader.classList.contains("panel-header")) return;
    
    panelHeader.style.justifyContent = "space-between";
    panelHeader.style.paddingRight = "10px";

    const expandBtn = document.createElement("span");
    expandBtn.classList.add('MaxBtn');
    expandBtn.innerHTML = '⛶';
    expandBtn.style.cursor = 'pointer';
    expandBtn.style.color = '#9ca3af';
    expandBtn.style.transition = 'color 0.2s';

    allButtons.push(expandBtn);

    expandBtn.onmouseenter = ()=> expandBtn.style.color = '#f3f4f6';
    expandBtn.onmouseleave = ()=> expandBtn.style.color = '#9ca3af';

    expandBtn.addEventListener('click', ()=>{
      let isMaximized = panel.classList.contains('isMaximized');

      if(!isMaximized){

        panels.forEach(p => p.classList.remove('isMaximized'))
        allButtons.forEach(btn => btn.innerHTML = '⛶');
        
        expandBtn.innerHTML = '✖';
        panel.classList.add('isMaximized');
        mainContainer?.classList.add('isFocused')
      }else {
        expandBtn.innerHTML = '⛶';
        panel.classList.remove('isMaximized');
        mainContainer?.classList.remove('isFocused')
      }
      
      setTimeout(()=>{
        window.dispatchEvent(new Event('resize'));
      },50);

    })

    panelHeader.appendChild(expandBtn);
  })
}


export default fullScreen;