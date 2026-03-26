package demo;

import org.springframework.web.bind.annotation.*;

/**
 * 付款单
 *
 * @author Roy
 */
@RestController
@RequestMapping("/api/v2/finance/payment_bill")
@Tag(name = "付款单管理")
public class FinPaymentBillController {

    @Operation(summary = "付款单列表")
    @PostMapping("/list")
    public String list() {
        return "";
    }
}
