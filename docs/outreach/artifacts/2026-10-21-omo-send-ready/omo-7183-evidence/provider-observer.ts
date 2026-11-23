import observer from './readiness-observer.ts'
import {installProviderCapture} from './provider-capture.ts'
export default {...observer,async server(input:any){installProviderCapture();return observer.server(input)}}
