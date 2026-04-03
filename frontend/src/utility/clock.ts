const updateClock = ():void => {
    const clockEl = document.getElementById('clock') as HTMLElement || null;
    if (clockEl) {
      const utcTime = new Date().toLocaleTimeString('en-GB', {
        timeZone: 'UTC',
        hour12: false
      });
      clockEl.textContent = `SYSTEM TIME: ${utcTime} UTC`;
    }
};

export default updateClock;